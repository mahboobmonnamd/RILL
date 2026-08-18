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
            Color::Indexed(n) if n < 16 => self.palette.ansi[n as usize],
            Color::Indexed(_) => self.palette.foreground,
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

    fn csi(&mut self, _params: &[u16], _intermediates: &[u8], ignore: bool, _action: char) {
        // Slice 3 owns CUP/ED. Slice 2 consumes CSI so high bytes inside
        // parameters do not become cells (ADR 0020 / S-VT).
        let _ = ignore;
    }

    fn esc(&mut self, _intermediates: &[u8], byte: u8) {
        match byte {
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
                if self.cursor_row > self.scroll_top {
                    self.cursor_row -= 1;
                }
            }
            _ => {}
        }
    }
}

fn pack(rgb: Rgb) -> u32 {
    (u32::from(rgb.r) << 24) | (u32::from(rgb.g) << 16) | (u32::from(rgb.b) << 8) | 0xff
}
