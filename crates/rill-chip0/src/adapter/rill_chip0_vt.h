#ifndef RILL_CHIP0_VT_H
#define RILL_CHIP0_VT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct RillVt RillVt;

typedef struct {
    uint32_t codepoint;
    uint32_t fg; /* RGBA8888 */
    uint32_t bg;
    uint16_t attrs; /* bit0 bold, bit1 underline, bit2 inverse */
    uint16_t _pad;
} RillPodCell;

typedef struct {
    uint16_t cols;
    uint16_t rows;
    uint16_t cursor_col;
    uint16_t cursor_row;
    uint8_t cursor_visible;
    uint8_t full_damage;
    uint16_t damage_row0;
    uint16_t damage_row1;
} RillPodHeader;

int rill_vt_new(RillVt **out, uint16_t cols, uint16_t rows);
void rill_vt_free(RillVt *vt);
void rill_vt_feed(RillVt *vt, const uint8_t *data, size_t len);
int rill_vt_resize(RillVt *vt, uint16_t cols, uint16_t rows, uint32_t cell_w, uint32_t cell_h);
int rill_vt_snapshot(RillVt *vt, RillPodHeader *hdr, RillPodCell **cells, size_t *ncells);
int rill_vt_repaint_bytes(RillVt *vt, uint8_t **bytes, size_t *len);
void rill_vt_reset(RillVt *vt);
void rill_vt_buf_free(uint8_t *ptr, size_t len);
void rill_vt_cells_free(RillPodCell *ptr);

#ifdef __cplusplus
}
#endif

#endif
