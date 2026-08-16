#ifndef RILL_FFI_H
#define RILL_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct Client RillClient;

typedef struct {
    uint32_t codepoint;
    uint32_t fg;
    uint32_t bg;
    uint16_t attrs;
    uint16_t _pad;
} RillPodCell;

typedef struct {
    uint16_t cols;
    uint16_t rows;
    uint16_t cursor_col;
    uint16_t cursor_row;
    uint8_t cursor_visible;
    uint8_t full_damage;
    const RillPodCell *cells;
    size_t ncells;
} RillPodGrid;

const char *rill_client_last_error(void);
RillClient *rill_client_connect(const char *socket);
void rill_client_free(RillClient *client);
int rill_client_send_input(RillClient *client, const uint8_t *bytes, size_t len);
int rill_client_resize(RillClient *client, uint16_t cols, uint16_t rows, uint16_t px_w, uint16_t px_h);
int rill_client_pump(RillClient *client);
int rill_client_alive(const RillClient *client);
const char *rill_client_font_family(const RillClient *client);
float rill_client_font_size(const RillClient *client);
int rill_client_snapshot(RillClient *client, RillPodGrid *out);
int rill_client_nfr_key(RillClient *client, uint32_t count, double *p95_ms, int *control_rpc, int *on_battery);

#ifdef __cplusplus
}
#endif

#endif
