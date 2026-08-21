//! Visible grid, cursor, damage. Does not parse bytes (SPEC-VT-SCREEN §1).

use crate::parser::Actions;
use rill_vt_types::{
    Color, Error, Palette, PodCell, PodGrid, Rgb, TerminalModeState, ATTR_WIDE_LEAD, ATTR_WIDE_TAIL,
};

#[derive(Clone, Copy)]
struct Cell {
    codepoint: u32,
    fg: Color,
    bg: Color,
    attrs: u16,
}

impl Cell {
    fn blank(bg: Color) -> Self {
        Self {
            codepoint: 32,
            fg: Color::Default,
            bg,
            attrs: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct SavedCursor {
    col: u16,
    row: u16,
    pending_wrap: bool,
    fg: Color,
    bg: Color,
    attrs: u16,
}

pub(crate) struct Screen {
    cols: u16,
    rows: u16,
    cells: Vec<Cell>,
    cursor_col: u16,
    cursor_row: u16,
    cursor_visible: bool,
    pending_wrap: bool,
    autowrap: bool,
    full_damage: bool,
    damage_row0: u16,
    damage_row1: u16,
    scroll_top: u16,
    scroll_bottom: u16,
    palette: Palette,
    pen_fg: Color,
    pen_bg: Color,
    pen_attrs: u16,
    saved_cursor: Option<SavedCursor>,
    alt_cursor: Option<SavedCursor>,
    saved_grid: Option<Vec<Cell>>,
    in_alt: bool,
    replies: Vec<u8>,
    replies_dropped: u32,
    discard_replies: bool,
    open_cluster: Option<OpenCluster>,
    grapheme_truncated: u32,
    application_cursor_keys: bool,
    application_keypad: bool,
    bracketed_paste: bool,
    mouse_x10: bool,
    mouse_button: bool,
    mouse_any: bool,
    mouse_sgr: bool,
    focus_events: bool,
    /// Primary-screen rows that left the visible grid once. Drained by
    /// `take_scrolled_off`. Not chip-owned scrollback (SPEC-VT-SCREEN §5).
    scrolled_off: Vec<Vec<Cell>>,
}

#[derive(Clone, Copy)]
struct OpenCluster {
    row: u16,
    col: u16,
    len: u8,
    after_zwj: bool,
    last_was_ri: bool,
    width: u8,
}

const RILL_GRAPHEME_MAX: u8 = 32;

fn resize_cells(old: &[Cell], old_cols: u16, old_rows: u16, cols: u16, rows: u16) -> Vec<Cell> {
    let mut next = vec![Cell::blank(Color::Default); usize::from(cols) * usize::from(rows)];
    let copy_cols = old_cols.min(cols);
    let copy_rows = old_rows.min(rows);
    for r in 0..copy_rows {
        for c in 0..copy_cols {
            let src = usize::from(r) * usize::from(old_cols) + usize::from(c);
            let dst = usize::from(r) * usize::from(cols) + usize::from(c);
            if src < old.len() {
                next[dst] = old[src];
            }
        }
    }
    next
}

impl Screen {
    pub(crate) fn new(cols: u16, rows: u16) -> Result<Self, Error> {
        if cols == 0 || rows == 0 {
            return Err(Error::Vt("empty grid"));
        }
        let n = usize::from(cols) * usize::from(rows);
        Ok(Self {
            cols,
            rows,
            cells: vec![Cell::blank(Color::Default); n],
            cursor_col: 0,
            cursor_row: 0,
            cursor_visible: true,
            pending_wrap: false,
            autowrap: true,
            full_damage: true,
            damage_row0: 0,
            damage_row1: 0,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            palette: Palette::vt_default(),
            pen_fg: Color::Default,
            pen_bg: Color::Default,
            pen_attrs: 0,
            saved_cursor: None,
            alt_cursor: None,
            saved_grid: None,
            in_alt: false,
            replies: Vec::new(),
            replies_dropped: 0,
            discard_replies: false,
            open_cluster: None,
            grapheme_truncated: 0,
            application_cursor_keys: false,
            application_keypad: false,
            bracketed_paste: false,
            mouse_x10: false,
            mouse_button: false,
            mouse_any: false,
            mouse_sgr: false,
            focus_events: false,
            scrolled_off: Vec::new(),
        })
    }

    pub(crate) fn mode_state(&self) -> TerminalModeState {
        TerminalModeState {
            application_cursor_keys: self.application_cursor_keys,
            application_keypad: self.application_keypad,
            bracketed_paste: self.bracketed_paste,
            mouse_x10: self.mouse_x10,
            mouse_button: self.mouse_button,
            mouse_any: self.mouse_any,
            mouse_sgr: self.mouse_sgr,
            focus_events: self.focus_events,
            alternate_screen: self.in_alt,
            cursor_visible: self.cursor_visible,
        }
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) -> Result<(), Error> {
        if cols == 0 || rows == 0 {
            return Err(Error::Vt("empty grid"));
        }
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("resize_clears_alt") {
            let next = resize_cells(&self.cells, self.cols, self.rows, cols, rows);
            self.cols = cols;
            self.rows = rows;
            self.cells = next;
            self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
            self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
            self.scroll_top = 0;
            self.scroll_bottom = rows.saturating_sub(1);
            self.pending_wrap = false;
            self.full_damage = true;
            self.saved_grid = None;
            self.saved_cursor = None;
            self.alt_cursor = None;
            self.in_alt = false;
            self.open_cluster = None;
            return Ok(());
        }
        let old_cols = self.cols;
        let old_rows = self.rows;
        let next = resize_cells(&self.cells, old_cols, old_rows, cols, rows);
        if let Some(saved) = self.saved_grid.take() {
            self.saved_grid = Some(resize_cells(&saved, old_cols, old_rows, cols, rows));
        }
        self.cols = cols;
        self.rows = rows;
        self.cells = next;
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.pending_wrap = false;
        self.full_damage = true;
        if let Some(c) = &mut self.saved_cursor {
            c.col = c.col.min(cols.saturating_sub(1));
            c.row = c.row.min(rows.saturating_sub(1));
        }
        if let Some(c) = &mut self.alt_cursor {
            c.col = c.col.min(cols.saturating_sub(1));
            c.row = c.row.min(rows.saturating_sub(1));
        }
        self.open_cluster = None;
        Ok(())
    }

    pub(crate) fn snapshot(&mut self) -> PodGrid {
        let mut cells: Vec<PodCell> = self
            .cells
            .iter()
            .map(|c| PodCell {
                codepoint: c.codepoint,
                fg: pack(self.resolve(c.fg, true)),
                bg: pack(self.resolve(c.bg, false)),
                attrs: c.attrs,
                _pad: 0,
            })
            .collect();

        if crate::mutate("unbounded_history") {
            cells.push(PodCell {
                codepoint: 32,
                fg: pack(self.palette.foreground),
                bg: pack(self.palette.background),
                attrs: 0,
                _pad: 0,
            });
        }

        let full_damage = self.full_damage || crate::mutate("always_full_damage");
        let damage_row0 = self.damage_row0;
        let damage_row1 = self.damage_row1;
        let grid = PodGrid {
            cols: self.cols,
            rows: self.rows,
            cursor_col: self.cursor_col,
            cursor_row: self.cursor_row,
            cursor_visible: self.cursor_visible,
            full_damage,
            damage_row0,
            damage_row1,
            default_fg: pack(self.palette.foreground),
            default_bg: pack(self.palette.background),
            grapheme_truncated: self.grapheme_truncated,
            replies_dropped: self.replies_dropped,
            cells,
        };
        self.full_damage = false;
        self.damage_row0 = 1;
        self.damage_row1 = 0;
        grid
    }

    pub(crate) fn color_at(&self, col: u16, row: u16) -> Option<(Color, Color)> {
        if col >= self.cols || row >= self.rows {
            return None;
        }
        let c = self.cells[self.idx(col, row)];
        Some((c.fg, c.bg))
    }

    pub(crate) fn set_palette(&mut self, palette: Palette) -> Result<(), Error> {
        if crate::mutate("skip_file_palette") {
            return Ok(());
        }
        self.palette = palette;
        self.full_damage = true;
        Ok(())
    }

    pub(crate) fn take_replies(&mut self) -> Result<Vec<u8>, Error> {
        Ok(std::mem::take(&mut self.replies))
    }

    pub(crate) fn take_scrolled_off(&mut self) -> Vec<Vec<PodCell>> {
        let rows = std::mem::take(&mut self.scrolled_off);
        rows.into_iter()
            .map(|row| row.into_iter().map(|c| self.pod_cell(c)).collect())
            .collect()
    }

    fn pod_cell(&self, c: Cell) -> PodCell {
        PodCell {
            codepoint: c.codepoint,
            fg: pack(self.resolve(c.fg, true)),
            bg: pack(self.resolve(c.bg, false)),
            attrs: c.attrs,
            _pad: 0,
        }
    }

    pub(crate) fn has_replies(&self) -> bool {
        !self.replies.is_empty()
    }

    pub(crate) fn cols(&self) -> u16 {
        self.cols
    }

    pub(crate) fn rows(&self) -> u16 {
        self.rows
    }

    pub(crate) fn set_discard_replies(&mut self, discard: bool) {
        self.discard_replies = discard;
    }

    pub(crate) fn repaint_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.in_alt {
            out.extend_from_slice(b"\x1b[?1049h");
        }
        out.extend_from_slice(b"\x1b[0m");
        let mut last_fg = Color::Default;
        let mut last_bg = Color::Default;
        let mut last_attrs: u16 = 0;
        for r in 0..self.rows {
            let mut last = 0u16;
            for c in 0..self.cols {
                let cell = self.cells[self.idx(c, r)];
                if !cell_is_blank(cell) {
                    last = c + 1;
                }
            }
            for c in 0..last {
                let cell = self.cells[self.idx(c, r)];
                if cell.fg != last_fg || cell.bg != last_bg || (cell.attrs & 0b111) != last_attrs {
                    out.extend_from_slice(b"\x1b[0m");
                    emit_attrs(&mut out, cell.attrs);
                    emit_color(&mut out, true, cell.fg);
                    emit_color(&mut out, false, cell.bg);
                    last_fg = cell.fg;
                    last_bg = cell.bg;
                    last_attrs = cell.attrs & 0b111;
                }
                if cell.attrs & ATTR_WIDE_TAIL != 0 && !crate::mutate("emit_wide_tails") {
                    continue;
                }
                push_codepoint(&mut out, cell.codepoint);
            }
            if r + 1 < self.rows {
                out.extend_from_slice(b"\r\n");
                last_fg = Color::Default;
                last_bg = Color::Default;
                last_attrs = 0;
                out.extend_from_slice(b"\x1b[0m");
            }
        }
        out.extend_from_slice(b"\x1b[");
        push_u16(&mut out, self.cursor_row.saturating_add(1));
        out.push(b';');
        push_u16(&mut out, self.cursor_col.saturating_add(1));
        out.push(b'H');
        if !self.cursor_visible {
            out.extend_from_slice(b"\x1b[?25l");
        }
        out
    }

    const REPLY_CAP: usize = 1024;

    fn enqueue_reply(&mut self, bytes: &[u8]) {
        if self.discard_replies {
            self.replies_dropped = self.replies_dropped.saturating_add(1);
            return;
        }
        if crate::mutate("no_reply") {
            return;
        }
        if crate::mutate("unbounded_replies") {
            self.replies.extend_from_slice(bytes);
            return;
        }
        if self.replies.len().saturating_add(bytes.len()) > Self::REPLY_CAP {
            self.replies_dropped = self.replies_dropped.saturating_add(1);
            return;
        }
        self.replies.extend_from_slice(bytes);
    }

    fn reply_primary_da(&mut self) {
        self.enqueue_reply(b"\x1b[?6c");
    }

    fn reply_secondary_da(&mut self) {
        self.enqueue_reply(b"\x1b[>0;0;0c");
    }

    fn reply_dsr_status(&mut self) {
        self.enqueue_reply(b"\x1b[0n");
    }

    fn reply_dsr_cursor(&mut self) {
        // Snapshot cursor, 1-based. Do not resolve pending wrap (SPEC-VT-REPLY §3).
        let row = self.cursor_row.saturating_add(1);
        let col = self.cursor_col.saturating_add(1);
        let mut buf = [0u8; 16];
        buf[0] = 0x1b;
        buf[1] = b'[';
        let mut i = 2;
        i += write_u16(&mut buf[i..], row);
        buf[i] = b';';
        i += 1;
        i += write_u16(&mut buf[i..], col);
        buf[i] = b'R';
        i += 1;
        self.enqueue_reply(&buf[..i]);
    }

    fn paint_indexed(&self, n: u16) -> Color {
        let idx = n.min(255) as u8;
        if crate::mutate("sgr_rgb_at_parse") {
            let rgb = crate::color::indexed(idx, &self.palette);
            Color::Rgb(rgb.r, rgb.g, rgb.b)
        } else {
            Color::Indexed(idx)
        }
    }

    fn apply_sgr(&mut self, params: &[u16]) {
        if crate::mutate("ignore_sgr") {
            return;
        }
        let params: &[u16] = if params.is_empty() { &[0] } else { params };
        let mut i = 0;
        while i < params.len() {
            let n = params[i];
            i += 1;
            match n {
                0 => {
                    self.pen_attrs = 0;
                    self.pen_fg = Color::Default;
                    self.pen_bg = Color::Default;
                }
                1 => self.pen_attrs |= 1,
                3 => {}
                4 => self.pen_attrs |= 2,
                7 => self.pen_attrs |= 4,
                22 => self.pen_attrs &= !1,
                24 => self.pen_attrs &= !2,
                27 => self.pen_attrs &= !4,
                30..=37 => self.pen_fg = self.paint_indexed(n - 30),
                90..=97 => self.pen_fg = self.paint_indexed(n - 90 + 8),
                40..=47 => self.pen_bg = self.paint_indexed(n - 40),
                100..=107 => self.pen_bg = self.paint_indexed(n - 100 + 8),
                39 => self.pen_fg = Color::Default,
                49 => self.pen_bg = Color::Default,
                38 | 48 => {
                    let fg = n == 38;
                    match params.get(i).copied() {
                        Some(5) => {
                            i += 1;
                            if let Some(idx) = params.get(i).copied() {
                                i += 1;
                                let c = self.paint_indexed(idx);
                                if fg {
                                    self.pen_fg = c;
                                } else {
                                    self.pen_bg = c;
                                }
                            }
                        }
                        Some(2) => {
                            i += 1;
                            if i + 2 < params.len() {
                                let r = params[i].min(255) as u8;
                                let g = params[i + 1].min(255) as u8;
                                let b = params[i + 2].min(255) as u8;
                                i += 3;
                                let c = Color::Rgb(r, g, b);
                                if fg {
                                    self.pen_fg = c;
                                } else {
                                    self.pen_bg = c;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn idx(&self, col: u16, row: u16) -> usize {
        usize::from(row) * usize::from(self.cols) + usize::from(col)
    }

    fn resolve(&self, color: Color, fg: bool) -> Rgb {
        match color {
            Color::Default => {
                if fg {
                    self.palette.foreground
                } else {
                    self.palette.background
                }
            }
            Color::Indexed(n) => crate::color::indexed(n, &self.palette),
            Color::Rgb(r, g, b) => Rgb { r, g, b },
        }
    }

    fn dirty(&mut self, row: u16) {
        if self.full_damage {
            return;
        }
        if self.damage_row0 > self.damage_row1 {
            self.damage_row0 = row;
            self.damage_row1 = row;
            return;
        }
        if row < self.damage_row0 {
            self.damage_row0 = row;
        }
        if row > self.damage_row1 {
            self.damage_row1 = row;
        }
    }

    fn clear_pending_wrap(&mut self) {
        self.pending_wrap = false;
    }

    fn index(&mut self) {
        if self.cursor_row < self.scroll_bottom {
            self.cursor_row += 1;
            return;
        }
        self.scroll_up();
    }

    fn scroll_up(&mut self) {
        let top = usize::from(self.scroll_top);
        let bot = usize::from(self.scroll_bottom);
        let cols = usize::from(self.cols);
        if !self.in_alt && self.scroll_top == 0 && cols > 0 {
            self.scrolled_off
                .push(self.cells[top * cols..top * cols + cols].to_vec());
        }
        for r in top..bot {
            let (src, dst) = {
                let d = r * cols;
                let s = (r + 1) * cols;
                (s, d)
            };
            self.cells.copy_within(src..src + cols, dst);
        }
        let start = bot * cols;
        for cell in &mut self.cells[start..start + cols] {
            *cell = Cell::blank(self.pen_bg);
        }
        self.full_damage = true;
    }

    fn scroll_down(&mut self) {
        let top = usize::from(self.scroll_top);
        let bot = usize::from(self.scroll_bottom);
        let cols = usize::from(self.cols);
        if bot > top {
            for r in (top + 1..=bot).rev() {
                let src = (r - 1) * cols;
                self.cells.copy_within(src..src + cols, r * cols);
            }
        }
        let start = top * cols;
        let blank = Cell::blank(self.pen_bg);
        for cell in &mut self.cells[start..start + cols] {
            *cell = blank;
        }
        self.full_damage = true;
    }

    fn reverse_index(&mut self) {
        if self.cursor_row > self.scroll_top {
            self.cursor_row -= 1;
            return;
        }
        self.scroll_down();
    }

    fn capture_cursor(&self) -> SavedCursor {
        SavedCursor {
            col: self.cursor_col,
            row: self.cursor_row,
            pending_wrap: self.pending_wrap,
            fg: self.pen_fg,
            bg: self.pen_bg,
            attrs: self.pen_attrs,
        }
    }

    fn restore_cursor(&mut self, saved: SavedCursor) {
        self.clear_pending_wrap();
        self.cursor_col = saved.col.min(self.last_col());
        self.cursor_row = saved.row.min(self.last_row());
        self.pending_wrap = saved.pending_wrap;
        self.pen_fg = saved.fg;
        self.pen_bg = saved.bg;
        self.pen_attrs = saved.attrs;
    }

    fn blank_grid(&self) -> Vec<Cell> {
        vec![Cell::blank(self.pen_bg); usize::from(self.cols) * usize::from(self.rows)]
    }

    fn enter_alt(&mut self, save_cursor: bool, clear: bool) {
        if crate::mutate("single_buffer") {
            if clear {
                self.erase_display(2);
                self.set_cursor(0, 0);
            }
            return;
        }
        if !self.in_alt {
            self.saved_grid = Some(self.cells.clone());
            if save_cursor {
                self.alt_cursor = Some(self.capture_cursor());
            } else {
                self.alt_cursor = None;
            }
        }
        if clear {
            self.cells = self.blank_grid();
            self.set_cursor(0, 0);
        }
        self.in_alt = true;
        self.full_damage = true;
    }

    fn leave_alt(&mut self, restore_cursor: bool) {
        if crate::mutate("single_buffer") {
            return;
        }
        if !self.in_alt {
            return;
        }
        if let Some(grid) = self.saved_grid.take() {
            self.cells = grid;
        }
        if restore_cursor {
            if let Some(c) = self.alt_cursor.take() {
                self.restore_cursor(c);
            }
        } else {
            self.alt_cursor = None;
        }
        self.in_alt = false;
        self.full_damage = true;
    }

    fn set_scroll_region(&mut self, params: &[u16]) {
        if crate::mutate("ignore_decstbm") {
            return;
        }
        let top = match params.first() {
            None | Some(0) => 1,
            Some(n) => *n,
        };
        let bot = match params.get(1) {
            None | Some(0) => self.rows,
            Some(n) => *n,
        };
        if top < 1 || bot > self.rows || top >= bot {
            return;
        }
        self.scroll_top = top - 1;
        self.scroll_bottom = bot - 1;
        self.set_cursor(0, 0);
    }

    fn private_mode(&mut self, params: &[u16], set: bool) {
        if crate::mutate("ignore_mode_updates") {
            return;
        }
        for p in params {
            match *p {
                1 => self.application_cursor_keys = set,
                7 => self.autowrap = set,
                25 => self.cursor_visible = set,
                2004 => self.bracketed_paste = set,
                1000 => self.mouse_x10 = set,
                1002 => self.mouse_button = set,
                1003 => self.mouse_any = set,
                1004 => self.focus_events = set,
                1006 => self.mouse_sgr = set,
                1047 => {
                    if set {
                        self.enter_alt(false, true);
                    } else {
                        self.leave_alt(false);
                    }
                }
                1049 => {
                    if set {
                        self.enter_alt(true, true);
                    } else {
                        self.leave_alt(true);
                    }
                }
                _ => {}
            }
        }
    }

    fn write_char(&mut self, c: char) {
        if crate::mutate("drop_print") {
            return;
        }
        if self.should_append(c) && self.append_to_cluster(c) {
            if is_regional_indicator(c) {
                self.maybe_expand_ri_pair();
            }
            return;
        }
        let width = self.cluster_width(c);
        if crate::mutate("eager_wrap")
            && self.autowrap
            && self.cursor_col + 1 == self.cols
            && !self.pending_wrap
        {
            // Wrap on *reaching* the last column: the last-column glyph goes
            // to the next row, so row 0 holds cols-1 characters.
            self.cursor_col = 0;
            self.index();
        } else {
            self.wrap_if_needed_width(width);
        }
        if self.cols < width || self.remaining_cols() < width {
            return;
        }
        let row = self.cursor_row;
        let col = self.cursor_col;
        self.smash_wide_at(col, row);
        if width == 2 {
            self.smash_wide_at(col + 1, row);
        }
        let sgr = self.pen_attrs & 0b111;
        let lead_i = self.idx(col, row);
        self.cells[lead_i] = Cell {
            codepoint: c as u32,
            fg: self.pen_fg,
            bg: self.pen_bg,
            attrs: sgr | if width == 2 { ATTR_WIDE_LEAD } else { 0 },
        };
        if width == 2 {
            let tail_i = self.idx(col + 1, row);
            self.cells[tail_i] = Cell {
                codepoint: c as u32,
                fg: self.pen_fg,
                bg: self.pen_bg,
                attrs: sgr | ATTR_WIDE_TAIL,
            };
        }
        self.open_cluster = Some(OpenCluster {
            row,
            col,
            len: 1,
            after_zwj: c == '\u{200d}',
            last_was_ri: is_regional_indicator(c),
            width: width as u8,
        });
        self.dirty(row);
        self.advance_after_width(width);
    }

    fn should_append(&self, c: char) -> bool {
        let Some(cl) = self.open_cluster else {
            return false;
        };
        is_cluster_continuer(c) || cl.after_zwj || (cl.last_was_ri && is_regional_indicator(c))
    }

    fn cluster_width(&self, c: char) -> u16 {
        if crate::mutate("narrow_cjk") {
            return 1;
        }
        if crate::east_asian_width::is_wide(c as u32) {
            2
        } else {
            1
        }
    }

    fn remaining_cols(&self) -> u16 {
        self.cols.saturating_sub(self.cursor_col)
    }

    fn wrap_if_needed_width(&mut self, width: u16) {
        let need = self.pending_wrap || self.remaining_cols() < width;
        if !need {
            return;
        }
        if !self.autowrap {
            self.pending_wrap = false;
            return;
        }
        self.pending_wrap = false;
        self.cursor_col = 0;
        self.index();
    }

    fn smash_wide_at(&mut self, col: u16, row: u16) {
        if crate::mutate("orphan_wide_tail") {
            return;
        }
        if col >= self.cols || row >= self.rows {
            return;
        }
        let i = self.idx(col, row);
        let attrs = self.cells[i].attrs;
        if attrs & ATTR_WIDE_LEAD != 0 && col + 1 < self.cols {
            let j = self.idx(col + 1, row);
            if self.cells[j].attrs & ATTR_WIDE_TAIL != 0 {
                self.cells[j] = Cell::blank(self.pen_bg);
            }
        }
        if attrs & ATTR_WIDE_TAIL != 0 && col > 0 {
            let j = self.idx(col - 1, row);
            if self.cells[j].attrs & ATTR_WIDE_LEAD != 0 {
                self.cells[j] = Cell::blank(self.pen_bg);
            }
        }
    }

    fn maybe_expand_ri_pair(&mut self) {
        let Some(mut cl) = self.open_cluster else {
            return;
        };
        if cl.width >= 2 || crate::mutate("narrow_cjk") {
            return;
        }
        let tail_col = cl.col.saturating_add(1);
        if tail_col >= self.cols || cl.row != self.cursor_row {
            return;
        }
        self.smash_wide_at(tail_col, cl.row);
        let lead_i = self.idx(cl.col, cl.row);
        let tail_i = self.idx(tail_col, cl.row);
        let lead = self.cells[lead_i];
        self.cells[lead_i].attrs = (lead.attrs & 0b111) | ATTR_WIDE_LEAD;
        self.cells[tail_i] = Cell {
            codepoint: lead.codepoint,
            fg: lead.fg,
            bg: lead.bg,
            attrs: (lead.attrs & 0b111) | ATTR_WIDE_TAIL,
        };
        cl.width = 2;
        self.open_cluster = Some(cl);
        self.dirty(cl.row);
        if self.cursor_col <= tail_col && self.cursor_row == cl.row {
            self.cursor_col = tail_col;
            self.advance_after_width(1);
        }
    }

    fn append_to_cluster(&mut self, c: char) -> bool {
        let Some(mut cl) = self.open_cluster else {
            return false;
        };
        let cap = if crate::mutate("fixed_grapheme_buf") {
            8
        } else {
            RILL_GRAPHEME_MAX
        };
        if cl.len >= cap {
            if !crate::mutate("fixed_grapheme_buf") {
                self.grapheme_truncated = self.grapheme_truncated.saturating_add(1);
            }
        } else {
            cl.len = cl.len.saturating_add(1);
        }
        cl.after_zwj = c == '\u{200d}';
        cl.last_was_ri = is_regional_indicator(c);
        self.dirty(cl.row);
        self.open_cluster = Some(cl);
        true
    }

    fn advance_after_width(&mut self, width: u16) {
        let next = self.cursor_col.saturating_add(width);
        if next < self.cols {
            self.cursor_col = next;
            self.pending_wrap = false;
        } else {
            self.cursor_col = self.last_col();
            self.pending_wrap = self.autowrap;
        }
    }

    fn last_col(&self) -> u16 {
        self.cols.saturating_sub(1)
    }

    fn last_row(&self) -> u16 {
        self.rows.saturating_sub(1)
    }

    fn set_cursor(&mut self, row: u16, col: u16) {
        self.clear_pending_wrap();
        self.open_cluster = None;
        self.cursor_row = row.min(self.last_row());
        self.cursor_col = col.min(self.last_col());
    }

    fn erase_span(&mut self, from: usize, to: usize) {
        let to = to.min(self.cells.len());
        let from = from.min(to);
        let cols = usize::from(self.cols).max(1);
        for i in from..to {
            let col = (i % cols) as u16;
            let row = (i / cols) as u16;
            self.smash_wide_at(col, row);
        }
        let blank = Cell::blank(self.pen_bg);
        for cell in &mut self.cells[from..to] {
            *cell = blank;
        }
        if from < to {
            let cols = usize::from(self.cols).max(1);
            let r0 = (from / cols) as u16;
            let r1 = ((to - 1) / cols) as u16;
            for r in r0..=r1 {
                self.dirty(r);
            }
        }
    }

    fn erase_display(&mut self, mode: u16) {
        let cur = self.idx(self.cursor_col, self.cursor_row);
        match mode {
            0 => self.erase_span(cur, self.cells.len()),
            1 => self.erase_span(0, cur.saturating_add(1)),
            _ => {
                self.erase_span(0, self.cells.len());
                self.full_damage = true;
            }
        }
    }

    fn erase_line(&mut self, mode: u16) {
        let row_start = self.idx(0, self.cursor_row);
        let row_end = row_start + usize::from(self.cols);
        let cur = self.idx(self.cursor_col, self.cursor_row);
        match mode {
            0 => self.erase_span(cur, row_end),
            1 => self.erase_span(row_start, cur.saturating_add(1)),
            _ => self.erase_span(row_start, row_end),
        }
    }

    fn erase_chars(&mut self, n: u16) {
        let n = n.min(self.cols.saturating_sub(self.cursor_col));
        let from = self.idx(self.cursor_col, self.cursor_row);
        self.erase_span(from, from + usize::from(n));
    }

    fn insert_lines(&mut self, n: u16) {
        if self.cursor_row < self.scroll_top || self.cursor_row > self.scroll_bottom {
            return;
        }
        let n = n.min(self.scroll_bottom.saturating_sub(self.cursor_row) + 1);
        if n == 0 {
            return;
        }
        let cols = usize::from(self.cols);
        let first = usize::from(self.cursor_row);
        let last = usize::from(self.scroll_bottom);
        let n_us = usize::from(n);
        if last >= first + n_us {
            for r in (first..=last - n_us).rev() {
                let src = r * cols;
                self.cells.copy_within(src..src + cols, src + n_us * cols);
            }
        }
        let blank = Cell::blank(self.pen_bg);
        for r in first..first + n_us {
            for cell in &mut self.cells[r * cols..r * cols + cols] {
                *cell = blank;
            }
        }
        for r in self.cursor_row..=self.scroll_bottom {
            self.dirty(r);
        }
    }

    fn delete_lines(&mut self, n: u16) {
        if self.cursor_row < self.scroll_top || self.cursor_row > self.scroll_bottom {
            return;
        }
        let n = n.min(self.scroll_bottom.saturating_sub(self.cursor_row) + 1);
        if n == 0 {
            return;
        }
        let cols = usize::from(self.cols);
        let first = usize::from(self.cursor_row);
        let last = usize::from(self.scroll_bottom);
        let n_us = usize::from(n);
        if last >= first + n_us {
            for r in first..=last - n_us {
                let src = (r + n_us) * cols;
                self.cells.copy_within(src..src + cols, r * cols);
            }
        }
        let blank = Cell::blank(self.pen_bg);
        for r in last + 1 - n_us..=last {
            for cell in &mut self.cells[r * cols..r * cols + cols] {
                *cell = blank;
            }
        }
        for r in self.cursor_row..=self.scroll_bottom {
            self.dirty(r);
        }
    }

    fn insert_cells(&mut self, n: u16) {
        let n = n.min(self.cols.saturating_sub(self.cursor_col));
        if n == 0 {
            return;
        }
        let cols = usize::from(self.cols);
        let col = usize::from(self.cursor_col);
        let n_us = usize::from(n);
        let row_start = self.idx(0, self.cursor_row);
        if col + n_us < cols {
            self.cells.copy_within(
                row_start + col..row_start + cols - n_us,
                row_start + col + n_us,
            );
        }
        let blank = Cell::blank(self.pen_bg);
        for cell in &mut self.cells[row_start + col..row_start + col + n_us] {
            *cell = blank;
        }
        self.dirty(self.cursor_row);
    }

    fn delete_cells(&mut self, n: u16) {
        let n = n.min(self.cols.saturating_sub(self.cursor_col));
        if n == 0 {
            return;
        }
        let row = self.cursor_row;
        for c in self.cursor_col..self.cursor_col + n {
            self.smash_wide_at(c, row);
        }
        let cols = usize::from(self.cols);
        let col = usize::from(self.cursor_col);
        let n_us = usize::from(n);
        let row_start = self.idx(0, self.cursor_row);
        if col + n_us < cols {
            self.cells
                .copy_within(row_start + col + n_us..row_start + cols, row_start + col);
        }
        let blank = Cell::blank(self.pen_bg);
        for cell in &mut self.cells[row_start + cols - n_us..row_start + cols] {
            *cell = blank;
        }
        self.dirty(self.cursor_row);
    }

    fn csi_dispatch(&mut self, params: &[u16], action: char) {
        match action {
            'A' => {
                let n = csi_n(params, 0, 1);
                self.set_cursor(self.cursor_row.saturating_sub(n), self.cursor_col);
            }
            'B' => {
                let n = csi_n(params, 0, 1);
                self.set_cursor(self.cursor_row.saturating_add(n), self.cursor_col);
            }
            'C' => {
                let n = csi_n(params, 0, 1);
                self.set_cursor(self.cursor_row, self.cursor_col.saturating_add(n));
            }
            'D' => {
                let n = csi_n(params, 0, 1);
                self.set_cursor(self.cursor_row, self.cursor_col.saturating_sub(n));
            }
            'E' => {
                let n = csi_n(params, 0, 1);
                self.set_cursor(self.cursor_row.saturating_add(n), 0);
            }
            'F' => {
                let n = csi_n(params, 0, 1);
                self.set_cursor(self.cursor_row.saturating_sub(n), 0);
            }
            'G' => {
                let col = csi_n(params, 0, 1).saturating_sub(1);
                self.set_cursor(self.cursor_row, col);
            }
            'H' | 'f' => {
                let row = csi_n(params, 0, 1).saturating_sub(1);
                let col = csi_n(params, 1, 1).saturating_sub(1);
                self.set_cursor(row, col);
            }
            'd' => {
                let row = csi_n(params, 0, 1).saturating_sub(1);
                self.set_cursor(row, self.cursor_col);
            }
            'J' => {
                if crate::mutate("noop_ed") {
                    return;
                }
                self.erase_display(csi_n(params, 0, 0));
            }
            'K' => self.erase_line(csi_n(params, 0, 0)),
            'X' => self.erase_chars(csi_n(params, 0, 1)),
            'L' => self.insert_lines(csi_n(params, 0, 1)),
            'M' => self.delete_lines(csi_n(params, 0, 1)),
            '@' => self.insert_cells(csi_n(params, 0, 1)),
            'P' => self.delete_cells(csi_n(params, 0, 1)),
            'r' => self.set_scroll_region(params),
            'S' => {
                let n = csi_n(params, 0, 1);
                for _ in 0..n {
                    self.scroll_up();
                }
            }
            'T' => {
                let n = csi_n(params, 0, 1);
                for _ in 0..n {
                    self.scroll_down();
                }
            }
            'm' => self.apply_sgr(params),
            'c' => {
                let n = params.first().copied().unwrap_or(0);
                if n == 0 {
                    self.reply_primary_da();
                }
            }
            'n' => match params.first().copied() {
                Some(6) => self.reply_dsr_cursor(),
                Some(5) => self.reply_dsr_status(),
                _ => {}
            },
            // REP (`b`) is a named miss (SPEC-VT-SCREEN §4).
            _ => {}
        }
    }
}

fn cell_is_blank(cell: Cell) -> bool {
    cell.codepoint == 32
        && cell.attrs == 0
        && matches!(cell.fg, Color::Default)
        && matches!(cell.bg, Color::Default)
}

fn push_codepoint(out: &mut Vec<u8>, cp: u32) {
    let Some(c) = char::from_u32(cp) else {
        return;
    };
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
}

fn push_u16(out: &mut Vec<u8>, n: u16) {
    let mut buf = [0u8; 5];
    let k = write_u16(&mut buf, n);
    out.extend_from_slice(&buf[..k]);
}

fn emit_attrs(out: &mut Vec<u8>, attrs: u16) {
    if attrs & 1 != 0 {
        out.extend_from_slice(b"\x1b[1m");
    }
    if attrs & 2 != 0 {
        out.extend_from_slice(b"\x1b[4m");
    }
    if attrs & 4 != 0 {
        out.extend_from_slice(b"\x1b[7m");
    }
}

fn emit_color(out: &mut Vec<u8>, fg: bool, color: Color) {
    match color {
        Color::Default => {}
        Color::Indexed(n) if n < 8 => {
            out.extend_from_slice(b"\x1b[");
            push_u16(out, (if fg { 30u16 } else { 40 }) + u16::from(n));
            out.push(b'm');
        }
        Color::Indexed(n) if n < 16 => {
            out.extend_from_slice(b"\x1b[");
            push_u16(
                out,
                (if fg { 90u16 } else { 100 }) + u16::from(n.saturating_sub(8)),
            );
            out.push(b'm');
        }
        Color::Indexed(n) => {
            out.extend_from_slice(if fg { b"\x1b[38;5;" } else { b"\x1b[48;5;" });
            push_u16(out, u16::from(n));
            out.push(b'm');
        }
        Color::Rgb(r, g, b) => {
            out.extend_from_slice(if fg { b"\x1b[38;2;" } else { b"\x1b[48;2;" });
            push_u16(out, u16::from(r));
            out.push(b';');
            push_u16(out, u16::from(g));
            out.push(b';');
            push_u16(out, u16::from(b));
            out.push(b'm');
        }
    }
}

fn write_u16(out: &mut [u8], n: u16) -> usize {
    let mut tmp = [0u8; 5];
    let mut x = n;
    let mut k = 0;
    loop {
        tmp[k] = b'0' + (x % 10) as u8;
        k += 1;
        x /= 10;
        if x == 0 {
            break;
        }
    }
    let mut w = 0;
    while k > 0 {
        k -= 1;
        out[w] = tmp[k];
        w += 1;
    }
    w
}

fn is_cluster_continuer(c: char) -> bool {
    let u = c as u32;
    crate::east_asian_width::is_mark(u)
        || c == '\u{200d}'
        || (0xfe00..=0xfe0f).contains(&u)
        || (0xe0100..=0xe01ef).contains(&u)
}

fn is_regional_indicator(c: char) -> bool {
    (0x1f1e6..=0x1f1ff).contains(&(c as u32))
}

fn csi_n(params: &[u16], i: usize, default: u16) -> u16 {
    match params.get(i) {
        None | Some(0) => default,
        Some(n) => *n,
    }
}

impl Actions for Screen {
    fn print(&mut self, c: char) {
        self.write_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 => {
                self.clear_pending_wrap();
                self.open_cluster = None;
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            0x09 => {
                self.clear_pending_wrap();
                self.open_cluster = None;
                let next = (self.cursor_col / 8 + 1) * 8;
                self.cursor_col = next.min(self.cols.saturating_sub(1));
            }
            0x0a..=0x0c => {
                if crate::mutate("ignore_crlf") {
                    return;
                }
                self.clear_pending_wrap();
                self.open_cluster = None;
                self.index();
            }
            0x0d => {
                if crate::mutate("ignore_crlf") {
                    return;
                }
                self.clear_pending_wrap();
                self.open_cluster = None;
                self.cursor_col = 0;
            }
            _ => {}
        }
    }

    fn csi(&mut self, params: &[u16], intermediates: &[u8], ignore: bool, action: char) {
        // ADR 0020 D4: overflow sets ignore; do not execute a truncated CSI.
        if crate::mutate("ignore_csi") || ignore {
            return;
        }
        if intermediates == b"?" && matches!(action, 'h' | 'l') {
            self.private_mode(params, action == 'h');
            return;
        }
        if intermediates == b">" && action == 'c' {
            self.reply_secondary_da();
            return;
        }
        if !intermediates.is_empty() {
            return;
        }
        self.csi_dispatch(params, action);
    }

    fn esc(&mut self, _intermediates: &[u8], byte: u8) {
        match byte {
            b'7' => self.saved_cursor = Some(self.capture_cursor()),
            b'8' => {
                if let Some(c) = self.saved_cursor {
                    self.restore_cursor(c);
                }
            }
            b'D' => {
                self.clear_pending_wrap();
                self.index();
            }
            b'E' => {
                self.clear_pending_wrap();
                self.cursor_col = 0;
                self.index();
            }
            b'M' => {
                self.clear_pending_wrap();
                self.reverse_index();
            }
            b'=' if _intermediates.is_empty() && !crate::mutate("ignore_mode_updates") => {
                self.application_keypad = true;
            }
            b'>' if _intermediates.is_empty() && !crate::mutate("ignore_mode_updates") => {
                self.application_keypad = false;
            }
            _ => {}
        }
    }
}

impl Screen {
    pub(crate) fn export_checkpoint(&self, ending_offset: u64) -> Result<Vec<u8>, Error> {
        if crate::mutate("empty_checkpoint") {
            return Ok(Vec::new());
        }
        let mut payload = Vec::new();
        put_u16(&mut payload, self.cols);
        put_u16(&mut payload, self.rows);
        put_u16(&mut payload, self.cursor_col);
        put_u16(&mut payload, self.cursor_row);
        put_u16(&mut payload, self.scroll_top);
        put_u16(&mut payload, self.scroll_bottom);
        put_u16(&mut payload, self.pen_attrs);
        put_u32(&mut payload, self.checkpoint_flags());
        put_color(&mut payload, self.pen_fg);
        put_color(&mut payload, self.pen_bg);
        put_palette(&mut payload, &self.palette);
        put_u32(&mut payload, self.grapheme_truncated);
        for cell in &self.cells {
            put_cell(&mut payload, cell);
        }
        match &self.saved_grid {
            Some(grid) => {
                payload.push(1);
                for cell in grid {
                    put_cell(&mut payload, cell);
                }
            }
            None => payload.push(0),
        }
        put_saved(&mut payload, self.saved_cursor);
        put_saved(&mut payload, self.alt_cursor);
        put_open_cluster(&mut payload, self.open_cluster);

        let mut out = Vec::with_capacity(22 + payload.len());
        out.extend_from_slice(MAGIC);
        put_u16(&mut out, VERSION);
        put_u64(&mut out, ending_offset);
        put_u64(&mut out, 0);
        out.extend_from_slice(&payload);
        let hash = if crate::mutate("constant_hash") {
            0
        } else {
            fnv1a64_with_zero_hash(&out)
        };
        out[HASH_OFF..HASH_OFF + 8].copy_from_slice(&hash.to_le_bytes());
        Ok(out)
    }

    pub(crate) fn import_checkpoint(&mut self, bytes: &[u8]) -> Result<u64, Error> {
        if bytes.len() < 22 {
            return Err(Error::Vt("truncated checkpoint"));
        }
        if bytes.get(..4) != Some(MAGIC) {
            return Err(Error::Vt("bad checkpoint magic"));
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != VERSION && !crate::mutate("accept_unknown_version") {
            return Err(Error::Vt("unsupported checkpoint version"));
        }
        let mut off_bytes = [0u8; 8];
        off_bytes.copy_from_slice(&bytes[6..14]);
        let ending_offset = u64::from_le_bytes(off_bytes);
        let mut hash_bytes = [0u8; 8];
        hash_bytes.copy_from_slice(&bytes[14..22]);
        let stored = u64::from_le_bytes(hash_bytes);
        if stored != fnv1a64_with_zero_hash(bytes) && !crate::mutate("accept_unknown_version") {
            return Err(Error::Vt("checkpoint hash mismatch"));
        }
        let mut rest = &bytes[22..];
        let cols = take_u16(&mut rest)?;
        let rows = take_u16(&mut rest)?;
        if cols == 0 || rows == 0 {
            return Err(Error::Vt("empty grid"));
        }
        let n = usize::from(cols)
            .checked_mul(usize::from(rows))
            .ok_or(Error::Vt("grid overflow"))?;
        let cursor_col = take_u16(&mut rest)?;
        let cursor_row = take_u16(&mut rest)?;
        let scroll_top = take_u16(&mut rest)?;
        let scroll_bottom = take_u16(&mut rest)?;
        let pen_attrs = take_u16(&mut rest)?;
        let flags = take_u32(&mut rest)?;
        let pen_fg = take_color(&mut rest)?;
        let pen_bg = take_color(&mut rest)?;
        let palette = take_palette(&mut rest)?;
        let grapheme_truncated = take_u32(&mut rest)?;
        let mut cells = Vec::with_capacity(n);
        for _ in 0..n {
            cells.push(take_cell(&mut rest)?);
        }
        let saved_grid = match take_u8(&mut rest)? {
            0 => None,
            1 => {
                let mut g = Vec::with_capacity(n);
                for _ in 0..n {
                    g.push(take_cell(&mut rest)?);
                }
                Some(g)
            }
            _ => return Err(Error::Vt("bad saved grid tag")),
        };
        let saved_cursor = take_saved(&mut rest)?;
        let alt_cursor = take_saved(&mut rest)?;
        let open_cluster = take_open_cluster(&mut rest)?;
        if !rest.is_empty() {
            return Err(Error::Vt("trailing checkpoint bytes"));
        }
        self.cols = cols;
        self.rows = rows;
        self.cells = cells;
        self.cursor_col = cursor_col.min(cols.saturating_sub(1));
        self.cursor_row = cursor_row.min(rows.saturating_sub(1));
        self.scroll_top = scroll_top;
        self.scroll_bottom = scroll_bottom;
        self.pen_attrs = pen_attrs;
        self.pen_fg = pen_fg;
        self.pen_bg = pen_bg;
        self.palette = palette;
        self.grapheme_truncated = grapheme_truncated;
        self.apply_checkpoint_flags(flags);
        self.saved_grid = saved_grid;
        self.saved_cursor = saved_cursor;
        self.alt_cursor = alt_cursor;
        self.open_cluster = open_cluster;
        self.full_damage = true;
        self.damage_row0 = 0;
        self.damage_row1 = 0;
        self.replies.clear();
        self.discard_replies = false;
        Ok(ending_offset)
    }

    fn checkpoint_flags(&self) -> u32 {
        let mut f = 0u32;
        if self.pending_wrap {
            f |= 1 << 0;
        }
        if self.autowrap {
            f |= 1 << 1;
        }
        if self.cursor_visible {
            f |= 1 << 2;
        }
        if self.in_alt {
            f |= 1 << 3;
        }
        if self.application_cursor_keys {
            f |= 1 << 4;
        }
        if self.application_keypad {
            f |= 1 << 5;
        }
        if self.bracketed_paste {
            f |= 1 << 6;
        }
        if self.mouse_x10 {
            f |= 1 << 7;
        }
        if self.mouse_button {
            f |= 1 << 8;
        }
        if self.mouse_any {
            f |= 1 << 9;
        }
        if self.mouse_sgr {
            f |= 1 << 10;
        }
        if self.focus_events {
            f |= 1 << 11;
        }
        f
    }

    fn apply_checkpoint_flags(&mut self, f: u32) {
        self.pending_wrap = f & (1 << 0) != 0;
        self.autowrap = f & (1 << 1) != 0;
        self.cursor_visible = f & (1 << 2) != 0;
        self.in_alt = f & (1 << 3) != 0;
        self.application_cursor_keys = f & (1 << 4) != 0;
        self.application_keypad = f & (1 << 5) != 0;
        self.bracketed_paste = f & (1 << 6) != 0;
        self.mouse_x10 = f & (1 << 7) != 0;
        self.mouse_button = f & (1 << 8) != 0;
        self.mouse_any = f & (1 << 9) != 0;
        self.mouse_sgr = f & (1 << 10) != 0;
        self.focus_events = f & (1 << 11) != 0;
    }
}

const MAGIC: &[u8] = b"R1CK";
const VERSION: u16 = 1;
const HASH_OFF: usize = 14;

fn fnv1a64_with_zero_hash(blob: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut h = OFFSET;
    for (i, b) in blob.iter().enumerate() {
        let byte = if (HASH_OFF..HASH_OFF + 8).contains(&i) {
            0
        } else {
            *b
        };
        h ^= u64::from(byte);
        h = h.wrapping_mul(PRIME);
    }
    h
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_color(out: &mut Vec<u8>, c: Color) {
    match c {
        Color::Default => out.push(0),
        Color::Indexed(i) => {
            out.push(1);
            out.push(i);
        }
        Color::Rgb(r, g, b) => {
            out.push(2);
            out.push(r);
            out.push(g);
            out.push(b);
        }
    }
}

fn put_rgb(out: &mut Vec<u8>, rgb: Rgb) {
    out.push(rgb.r);
    out.push(rgb.g);
    out.push(rgb.b);
}

fn put_palette(out: &mut Vec<u8>, p: &Palette) {
    for rgb in p.ansi {
        put_rgb(out, rgb);
    }
    put_rgb(out, p.foreground);
    put_rgb(out, p.background);
    put_rgb(out, p.cursor);
}

fn put_cell(out: &mut Vec<u8>, c: &Cell) {
    put_u32(out, c.codepoint);
    put_u16(out, c.attrs);
    put_color(out, c.fg);
    put_color(out, c.bg);
}

fn put_saved(out: &mut Vec<u8>, s: Option<SavedCursor>) {
    match s {
        None => out.push(0),
        Some(c) => {
            out.push(1);
            put_u16(out, c.col);
            put_u16(out, c.row);
            out.push(u8::from(c.pending_wrap));
            put_color(out, c.fg);
            put_color(out, c.bg);
            put_u16(out, c.attrs);
        }
    }
}

fn take_u8(rest: &mut &[u8]) -> Result<u8, Error> {
    let (b, tail) = rest
        .split_first()
        .ok_or(Error::Vt("truncated checkpoint"))?;
    *rest = tail;
    Ok(*b)
}

fn take_u16(rest: &mut &[u8]) -> Result<u16, Error> {
    if rest.len() < 2 {
        return Err(Error::Vt("truncated checkpoint"));
    }
    let v = u16::from_le_bytes([rest[0], rest[1]]);
    *rest = &rest[2..];
    Ok(v)
}

fn take_u32(rest: &mut &[u8]) -> Result<u32, Error> {
    if rest.len() < 4 {
        return Err(Error::Vt("truncated checkpoint"));
    }
    let mut raw = [0u8; 4];
    raw.copy_from_slice(&rest[..4]);
    let v = u32::from_le_bytes(raw);
    *rest = &rest[4..];
    Ok(v)
}

fn take_color(rest: &mut &[u8]) -> Result<Color, Error> {
    match take_u8(rest)? {
        0 => Ok(Color::Default),
        1 => Ok(Color::Indexed(take_u8(rest)?)),
        2 => {
            let r = take_u8(rest)?;
            let g = take_u8(rest)?;
            let b = take_u8(rest)?;
            Ok(Color::Rgb(r, g, b))
        }
        _ => Err(Error::Vt("bad colour tag")),
    }
}

fn take_rgb(rest: &mut &[u8]) -> Result<Rgb, Error> {
    Ok(Rgb {
        r: take_u8(rest)?,
        g: take_u8(rest)?,
        b: take_u8(rest)?,
    })
}

fn take_palette(rest: &mut &[u8]) -> Result<Palette, Error> {
    let mut ansi = [Rgb { r: 0, g: 0, b: 0 }; 16];
    for slot in &mut ansi {
        *slot = take_rgb(rest)?;
    }
    Ok(Palette {
        ansi,
        foreground: take_rgb(rest)?,
        background: take_rgb(rest)?,
        cursor: take_rgb(rest)?,
    })
}

fn take_cell(rest: &mut &[u8]) -> Result<Cell, Error> {
    Ok(Cell {
        codepoint: take_u32(rest)?,
        attrs: take_u16(rest)?,
        fg: take_color(rest)?,
        bg: take_color(rest)?,
    })
}

fn take_saved(rest: &mut &[u8]) -> Result<Option<SavedCursor>, Error> {
    match take_u8(rest)? {
        0 => Ok(None),
        1 => Ok(Some(SavedCursor {
            col: take_u16(rest)?,
            row: take_u16(rest)?,
            pending_wrap: take_u8(rest)? != 0,
            fg: take_color(rest)?,
            bg: take_color(rest)?,
            attrs: take_u16(rest)?,
        })),
        _ => Err(Error::Vt("bad saved cursor tag")),
    }
}

fn put_open_cluster(out: &mut Vec<u8>, o: Option<OpenCluster>) {
    match o {
        None => out.push(0),
        Some(c) => {
            out.push(1);
            put_u16(out, c.row);
            put_u16(out, c.col);
            out.push(c.len);
            out.push(u8::from(c.after_zwj));
            out.push(u8::from(c.last_was_ri));
            out.push(c.width);
        }
    }
}

fn take_open_cluster(rest: &mut &[u8]) -> Result<Option<OpenCluster>, Error> {
    match take_u8(rest)? {
        0 => Ok(None),
        1 => Ok(Some(OpenCluster {
            row: take_u16(rest)?,
            col: take_u16(rest)?,
            len: take_u8(rest)?,
            after_zwj: take_u8(rest)? != 0,
            last_was_ri: take_u8(rest)? != 0,
            width: take_u8(rest)?,
        })),
        _ => Err(Error::Vt("bad open cluster tag")),
    }
}

fn pack(rgb: Rgb) -> u32 {
    (u32::from(rgb.r) << 24) | (u32::from(rgb.g) << 16) | (u32::from(rgb.b) << 8) | 0xff
}
