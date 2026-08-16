/* Adapter only. Ghostty FFI types stay in this file. */

#define GHOSTTY_STATIC
#include "rill_chip0_vt.h"

#include <ghostty/vt.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

struct RillVt {
    GhosttyTerminal term;
    GhosttyRenderState render;
    GhosttyRenderStateRowIterator rows;
    GhosttyRenderStateRowCells cells;
};

static uint32_t rgba(GhosttyColorRgb c) {
    return ((uint32_t)c.r << 24) | ((uint32_t)c.g << 16) | ((uint32_t)c.b << 8) | 0xffu;
}

int rill_vt_new(RillVt **out, uint16_t cols, uint16_t rows) {
    if (!out || cols == 0 || rows == 0) {
        return -1;
    }
    RillVt *vt = calloc(1, sizeof(RillVt));
    if (!vt) {
        return -1;
    }
    if (ghostty_terminal_new(NULL, &vt->term, cols, rows) != GHOSTTY_SUCCESS) {
        free(vt);
        return -1;
    }
    if (ghostty_render_state_new(NULL, &vt->render) != GHOSTTY_SUCCESS) {
        ghostty_terminal_free(vt->term);
        free(vt);
        return -1;
    }
    if (ghostty_render_state_row_iterator_new(NULL, &vt->rows) != GHOSTTY_SUCCESS) {
        ghostty_render_state_free(vt->render);
        ghostty_terminal_free(vt->term);
        free(vt);
        return -1;
    }
    if (ghostty_render_state_row_cells_new(NULL, &vt->cells) != GHOSTTY_SUCCESS) {
        ghostty_render_state_row_iterator_free(vt->rows);
        ghostty_render_state_free(vt->render);
        ghostty_terminal_free(vt->term);
        free(vt);
        return -1;
    }
    *out = vt;
    return 0;
}

void rill_vt_free(RillVt *vt) {
    if (!vt) {
        return;
    }
    ghostty_render_state_row_cells_free(vt->cells);
    ghostty_render_state_row_iterator_free(vt->rows);
    ghostty_render_state_free(vt->render);
    ghostty_terminal_free(vt->term);
    free(vt);
}

void rill_vt_feed(RillVt *vt, const uint8_t *data, size_t len) {
    if (!vt || !data || len == 0) {
        return;
    }
    ghostty_terminal_vt_write(vt->term, data, len);
}

int rill_vt_resize(RillVt *vt, uint16_t cols, uint16_t rows, uint32_t cell_w, uint32_t cell_h) {
    if (!vt) {
        return -1;
    }
    if (ghostty_terminal_resize(vt->term, cols, rows, cell_w, cell_h) != GHOSTTY_SUCCESS) {
        return -1;
    }
    return 0;
}

void rill_vt_reset(RillVt *vt) {
    if (!vt) {
        return;
    }
    ghostty_terminal_reset(vt->term);
}

int rill_vt_snapshot(RillVt *vt, RillPodHeader *hdr, RillPodCell **cells, size_t *ncells) {
    if (!vt || !hdr || !cells || !ncells) {
        return -1;
    }
    if (ghostty_render_state_update(vt->render, vt->term) != GHOSTTY_SUCCESS) {
        return -1;
    }

    uint16_t cols = 0;
    uint16_t rows = 0;
    ghostty_render_state_get(vt->render, GHOSTTY_RENDER_STATE_DATA_COLS, &cols);
    ghostty_render_state_get(vt->render, GHOSTTY_RENDER_STATE_DATA_ROWS, &rows);

    GhosttyColorRgb def_fg = {0xcc, 0xcc, 0xcc};
    GhosttyColorRgb def_bg = {0x12, 0x12, 0x12};
    ghostty_render_state_get(vt->render, GHOSTTY_RENDER_STATE_DATA_COLOR_FOREGROUND, &def_fg);
    ghostty_render_state_get(vt->render, GHOSTTY_RENDER_STATE_DATA_COLOR_BACKGROUND, &def_bg);

    GhosttyRenderStateDirty dirty = GHOSTTY_RENDER_STATE_DIRTY_FULL;
    ghostty_render_state_get(vt->render, GHOSTTY_RENDER_STATE_DATA_DIRTY, &dirty);

    GhosttyRenderStateCursor cursor = GHOSTTY_INIT_SIZED(GhosttyRenderStateCursor);
    ghostty_render_state_get(vt->render, GHOSTTY_RENDER_STATE_DATA_CURSOR, &cursor);

    size_t n = (size_t)cols * (size_t)rows;
    RillPodCell *grid = calloc(n, sizeof(RillPodCell));
    if (!grid) {
        return -1;
    }
    for (size_t i = 0; i < n; i++) {
        grid[i].codepoint = 32;
        grid[i].fg = rgba(def_fg);
        grid[i].bg = rgba(def_bg);
    }

    uint16_t d0 = 0;
    uint16_t d1 = rows == 0 ? 0 : (uint16_t)(rows - 1);
    int first_dirty = 1;

    if (ghostty_render_state_get(vt->render, GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR, &vt->rows)
        != GHOSTTY_SUCCESS) {
        free(grid);
        return -1;
    }

    uint16_t y = 0;
    while (ghostty_render_state_row_iterator_next(vt->rows)) {
        if (ghostty_render_state_row_get(
                vt->rows, GHOSTTY_RENDER_STATE_ROW_DATA_CELLS, &vt->cells)
            != GHOSTTY_SUCCESS) {
            y++;
            continue;
        }
        uint16_t x = 0;
        while (ghostty_render_state_row_cells_next(vt->cells) && x < cols) {
            uint32_t glen = 0;
            ghostty_render_state_row_cells_get(
                vt->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN, &glen);
            uint32_t cp = 32;
            if (glen > 0) {
                uint32_t buf[8];
                uint32_t take = glen < 8 ? glen : 8;
                ghostty_render_state_row_cells_get(
                    vt->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF, buf);
                cp = buf[0];
                (void)take;
            }
            GhosttyColorRgb fg = def_fg;
            GhosttyColorRgb bg = def_bg;
            if (ghostty_render_state_row_cells_get(
                    vt->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR, &fg)
                != GHOSTTY_SUCCESS) {
                fg = def_fg;
            }
            if (ghostty_render_state_row_cells_get(
                    vt->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR, &bg)
                != GHOSTTY_SUCCESS) {
                bg = def_bg;
            }
            GhosttyStyle style = GHOSTTY_INIT_SIZED(GhosttyStyle);
            ghostty_render_state_row_cells_get(
                vt->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE, &style);
            uint16_t attrs = 0;
            if (style.bold) {
                attrs |= 1;
            }
            if (style.underline) {
                attrs |= 2;
            }
            if (style.inverse) {
                attrs |= 4;
            }
            size_t idx = (size_t)y * (size_t)cols + (size_t)x;
            grid[idx].codepoint = cp;
            grid[idx].fg = rgba(fg);
            grid[idx].bg = rgba(bg);
            grid[idx].attrs = attrs;
            x++;
        }
        bool row_dirty = false;
        ghostty_render_state_row_get(
            vt->rows, GHOSTTY_RENDER_STATE_ROW_DATA_DIRTY, &row_dirty);
        if (row_dirty || dirty == GHOSTTY_RENDER_STATE_DIRTY_FULL) {
            if (first_dirty) {
                d0 = y;
                first_dirty = 0;
            }
            d1 = y;
        }
        y++;
        if (y >= rows) {
            break;
        }
    }

    hdr->cols = cols;
    hdr->rows = rows;
    hdr->cursor_col = cursor.viewport_has_value ? cursor.viewport_x : 0;
    hdr->cursor_row = cursor.viewport_has_value ? cursor.viewport_y : 0;
    hdr->cursor_visible = cursor.visible && cursor.viewport_has_value;
    hdr->full_damage = dirty == GHOSTTY_RENDER_STATE_DIRTY_FULL;
    hdr->damage_row0 = d0;
    hdr->damage_row1 = d1;
    *cells = grid;
    *ncells = n;
    ghostty_render_state_clean(vt->render);
    return 0;
}

int rill_vt_repaint_bytes(RillVt *vt, uint8_t **bytes, size_t *len) {
    if (!vt || !bytes || !len) {
        return -1;
    }
    GhosttyFormatterTerminalOptions opts = GHOSTTY_INIT_SIZED(GhosttyFormatterTerminalOptions);
    opts.emit = GHOSTTY_FORMATTER_FORMAT_VT;
    opts.unwrap = false;
    opts.trim = false;
    opts.extra = GHOSTTY_INIT_SIZED(GhosttyFormatterTerminalExtra);
    opts.extra.screen = GHOSTTY_INIT_SIZED(GhosttyFormatterScreenExtra);
    opts.extra.screen.cursor = true;
    opts.extra.screen.style = true;

    GhosttyFormatter formatter = NULL;
    if (ghostty_formatter_terminal_new(NULL, &formatter, vt->term, opts) != GHOSTTY_SUCCESS) {
        return -1;
    }
    uint8_t *buf = NULL;
    size_t n = 0;
    GhosttyResult r = ghostty_formatter_format_alloc(formatter, NULL, &buf, &n);
    ghostty_formatter_free(formatter);
    if (r != GHOSTTY_SUCCESS) {
        return -1;
    }
    *bytes = buf;
    *len = n;
    return 0;
}

void rill_vt_buf_free(uint8_t *ptr, size_t len) {
    ghostty_free(NULL, ptr, len);
}

void rill_vt_cells_free(RillPodCell *ptr) {
    free(ptr);
}
