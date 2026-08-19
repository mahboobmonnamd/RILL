//! Visible grid, cursor, damage. Does not parse bytes (SPEC-VT-SCREEN §1).

use crate::parser::Actions;
use rill_vt_types::{Color, Error, Palette, PodCell, PodGrid, Rgb};

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
        })
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) -> Result<(), Error> {
        if cols == 0 || rows == 0 {
            return Err(Error::Vt("empty grid"));
        }
        let mut next = vec![Cell::blank(Color::Default); usize::from(cols) * usize::from(rows)];
        let copy_cols = self.cols.min(cols);
        let copy_rows = self.rows.min(rows);
        for r in 0..copy_rows {
            for c in 0..copy_cols {
                next[usize::from(r) * usize::from(cols) + usize::from(c)] =
                    self.cells[self.idx(c, r)];
            }
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
        self.saved_grid = None;
        self.saved_cursor = None;
        self.alt_cursor = None;
        self.in_alt = false;
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
            grapheme_truncated: 0,
            replies_dropped: 0,
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

    fn wrap_if_needed(&mut self) {
        if !self.pending_wrap || !self.autowrap {
            self.pending_wrap = false;
            return;
        }
        self.pending_wrap = false;
        self.cursor_col = 0;
        self.index();
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
        for p in params {
            match *p {
                7 => self.autowrap = set,
                25 => self.cursor_visible = set,
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
            self.wrap_if_needed();
        }
        let i = self.idx(self.cursor_col, self.cursor_row);
        self.cells[i] = Cell {
            codepoint: c as u32,
            fg: self.pen_fg,
            bg: self.pen_bg,
            attrs: self.pen_attrs,
        };
        self.dirty(self.cursor_row);
        if self.cursor_col + 1 < self.cols {
            self.cursor_col += 1;
            self.pending_wrap = false;
        } else {
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
        self.cursor_row = row.min(self.last_row());
        self.cursor_col = col.min(self.last_col());
    }

    fn erase_span(&mut self, from: usize, to: usize) {
        let to = to.min(self.cells.len());
        let from = from.min(to);
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
            // REP (`b`) is a named miss (SPEC-VT-SCREEN §4). DA is Slice 6.
            _ => {}
        }
    }
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
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            0x09 => {
                self.clear_pending_wrap();
                let next = (self.cursor_col / 8 + 1) * 8;
                self.cursor_col = next.min(self.cols.saturating_sub(1));
            }
            0x0a..=0x0c => {
                if crate::mutate("ignore_crlf") {
                    return;
                }
                self.clear_pending_wrap();
                self.index();
            }
            0x0d => {
                if crate::mutate("ignore_crlf") {
                    return;
                }
                self.clear_pending_wrap();
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
            _ => {}
        }
    }
}

fn pack(rgb: Rgb) -> u32 {
    (u32::from(rgb.r) << 24) | (u32::from(rgb.g) << 16) | (u32::from(rgb.b) << 8) | 0xff
}
