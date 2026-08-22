//! Host viewport over Chip 1 live grid plus drained scrolled-off rows.
//! Chip 1 does not keep a history ring (SPEC-VT-SCREEN §5).

use rill_vt_types::{PodCell, PodGrid};

const MAX_HISTORY_ROWS: usize = 10_000;

pub fn ingest_scrolled(history: &mut Vec<Vec<PodCell>>, rows: Vec<Vec<PodCell>>) {
    for row in rows {
        history.push(row);
        if history.len() > MAX_HISTORY_ROWS {
            history.remove(0);
        }
    }
}

pub fn clamp_offset(history_len: usize, live_rows: u16, offset: u32) -> u32 {
    let max = history_len as u32;
    let _ = live_rows;
    offset.min(max)
}

/// Typing returns the presenter to the live tail. Mutation `keep_scroll_on_input`
/// leaves a wheel offset in place (viewport looks frozen).
pub fn follow_live_after_input(offset: u32) -> u32 {
    if std::env::var("RILL_MUTATE").as_deref() == Ok("keep_scroll_on_input") {
        return offset;
    }
    0
}

/// A skippable Chip 1 snapshot (empty damage) must not keep stale GPU rows
/// after the host viewport moved. Mutation `honor_empty_damage` leaves it skippable.
pub fn mark_full_if_viewport_jumped(grid: &mut PodGrid, jumped: bool) {
    if std::env::var("RILL_MUTATE").as_deref() == Ok("honor_empty_damage") {
        return;
    }
    if jumped {
        grid.full_damage = true;
    }
}

/// Paint `live.rows` of cells ending `offset` rows above the live tail.
pub fn compose_viewport(history: &[Vec<PodCell>], live: &PodGrid, offset: u32) -> PodGrid {
    if offset == 0 || history.is_empty() {
        return live.clone();
    }
    let cols = live.cols as usize;
    let rows = live.rows as usize;
    let mut stack: Vec<&[PodCell]> = Vec::with_capacity(history.len() + rows);
    for h in history {
        stack.push(h.as_slice());
    }
    for r in 0..live.rows {
        let start = r as usize * cols;
        if start + cols <= live.cells.len() {
            stack.push(&live.cells[start..start + cols]);
        }
    }
    let off = clamp_offset(history.len(), live.rows, offset) as usize;
    let end = stack.len().saturating_sub(off);
    let start = end.saturating_sub(rows);
    let mut cells = Vec::with_capacity(rows * cols);
    let blank = PodCell {
        codepoint: 32,
        fg: live.default_fg,
        bg: live.default_bg,
        attrs: 0,
        _pad: 0,
    };
    for i in start..start + rows {
        if let Some(row) = stack.get(i) {
            if row.len() >= cols {
                cells.extend_from_slice(&row[..cols]);
            } else {
                cells.extend_from_slice(row);
                cells.resize(cells.len() + (cols - row.len()), blank);
            }
        } else {
            cells.extend(std::iter::repeat(blank).take(cols));
        }
    }
    PodGrid {
        cols: live.cols,
        rows: live.rows,
        cursor_col: live.cursor_col,
        cursor_row: live.cursor_row,
        cursor_visible: offset == 0 && live.cursor_visible,
        full_damage: true,
        damage_row0: 0,
        damage_row1: live.rows.saturating_sub(1),
        default_fg: live.default_fg,
        default_bg: live.default_bg,
        grapheme_truncated: live.grapheme_truncated,
        replies_dropped: live.replies_dropped,
        cells,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_vt_types::TerminalEmulation;
    use vt_engine::VtEngine;

    fn blank_row(n: usize, cp: u32) -> Vec<PodCell> {
        vec![
            PodCell {
                codepoint: cp,
                fg: 0,
                bg: 0,
                attrs: 0,
                _pad: 0,
            };
            n
        ]
    }

    /// T-SCROLL-OFFSCREEN. Lines that left the Chip 1 grid must be visible
    /// after a host viewport offset. Mutation `ignore_wheel` keeps live row 0.
    ///
    /// Required mutation: `RILL_MUTATE=ignore_wheel`.
    #[test]
    fn t_scroll_wheel_reveals_lines_that_left_the_grid() {
        let mut vt = VtEngine::new(8, 3).expect("vt");
        vt.feed(b"MARK\r\n").expect("mark");
        vt.feed(b"aaaa\r\nbbbb\r\ncccc\r\n").expect("fill");
        let live = vt.snapshot().expect("live");
        assert_ne!(
            live.cell(0, 0).map(|c| c.codepoint),
            Some(u32::from(b'M')),
            "oracle: MARK must have left the live grid"
        );
        let drained = vt.take_scrolled_off();
        assert!(
            !drained.is_empty(),
            "chip must report scrolled-off primary rows once"
        );
        let mut history = Vec::new();
        ingest_scrolled(&mut history, drained);
        let offset = if crate_mutate_ignore_wheel() {
            0
        } else {
            history.len() as u32
        };
        let view = compose_viewport(&history, &live, offset);
        let got = view.cell(0, 0).map(|c| c.codepoint);
        assert_eq!(
            got,
            Some(u32::from(b'M')),
            "wheel/offset must show MARK on painted row 0"
        );
    }

    fn crate_mutate_ignore_wheel() -> bool {
        std::env::var("RILL_MUTATE").as_deref() == Ok("ignore_wheel")
    }

    /// Bug: a wheel offset stayed after keystrokes, so the live prompt looked stuck.
    /// Required mutation: `RILL_MUTATE=keep_scroll_on_input`.
    #[test]
    fn t_keystroke_returns_scrolled_viewport_to_live() {
        let live = PodGrid {
            cols: 2,
            rows: 1,
            cursor_col: 0,
            cursor_row: 0,
            cursor_visible: true,
            full_damage: false,
            damage_row0: 1,
            damage_row1: 0,
            default_fg: 0,
            default_bg: 0,
            grapheme_truncated: 0,
            replies_dropped: 0,
            cells: blank_row(2, b'Z' as u32),
        };
        let history = vec![blank_row(2, b'H' as u32)];
        let after_wheel = 1;
        let scrolled = compose_viewport(&history, &live, after_wheel);
        assert_eq!(
            scrolled.cell(0, 0).map(|c| c.codepoint),
            Some(b'H' as u32),
            "precondition: offset must show history"
        );
        let offset = follow_live_after_input(after_wheel);
        let view = compose_viewport(&history, &live, offset);
        assert_eq!(
            view.cell(0, 0).map(|c| c.codepoint),
            Some(b'Z' as u32),
            "input must paint the live tail, not the wheel offset"
        );
    }

    /// Bug: follow-live / wheel used a skippable VT snapshot so Metal kept
    /// history rows while the caret moved. Required mutation: `honor_empty_damage`.
    #[test]
    fn t_viewport_jump_paints_full_grid_not_stale_rows() {
        let mut grid = PodGrid {
            cols: 2,
            rows: 1,
            cursor_col: 0,
            cursor_row: 0,
            cursor_visible: true,
            full_damage: false,
            damage_row0: 1,
            damage_row1: 0,
            default_fg: 0,
            default_bg: 0,
            grapheme_truncated: 0,
            replies_dropped: 0,
            cells: blank_row(2, b'P' as u32),
        };
        assert!(
            grid.damage_row0 > grid.damage_row1,
            "precondition: skippable damage"
        );
        mark_full_if_viewport_jumped(&mut grid, true);
        assert!(
            grid.full_damage,
            "viewport jump must force a full instance rebuild"
        );
    }

    #[test]
    fn compose_zero_offset_is_live() {
        let live = PodGrid {
            cols: 2,
            rows: 1,
            cursor_col: 0,
            cursor_row: 0,
            cursor_visible: true,
            full_damage: false,
            damage_row0: 1,
            damage_row1: 0,
            default_fg: 0,
            default_bg: 0,
            grapheme_truncated: 0,
            replies_dropped: 0,
            cells: blank_row(2, b'Z' as u32),
        };
        let history = vec![blank_row(2, b'H' as u32)];
        let v = compose_viewport(&history, &live, 0);
        assert_eq!(v.cell(0, 0).map(|c| c.codepoint), Some(b'Z' as u32));
    }
}
