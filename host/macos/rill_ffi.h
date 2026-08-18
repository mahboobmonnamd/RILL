#ifndef RILL_FFI_H
#define RILL_FFI_H

/* C ABI exported by crates/rill-host. No PTY symbols: the GUI attaches to a
 * session it did not create (PRD FR-SPAWN, SPEC-DISPLAY §1). */

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct Client RillClient;

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
    uint32_t grapheme_truncated;
    const RillPodCell *cells;
    size_t ncells;
} RillPodGrid;

const char *rill_client_last_error(void);

RillClient *rill_client_connect(const char *socket);
void rill_client_free(RillClient *client);

/* Attach socket fd, for arming a dispatch_source. The warm path is
 * event-driven; there is no timer (ADR 0003 D2). */
int rill_client_socket_fd(const RillClient *client);

int rill_client_send_input(RillClient *client, const uint8_t *bytes, size_t len);
int rill_client_resize(RillClient *client, uint16_t cols, uint16_t rows,
                       uint16_t px_w, uint16_t px_h);

/* Returns bytes fed this turn, or -1 on error. */
ptrdiff_t rill_client_pump(RillClient *client);

int rill_client_alive(const RillClient *client);
int rill_client_exit_status(const RillClient *client); /* INT32_MIN while alive */

const char *rill_client_font_family(const RillClient *client);
const char *rill_client_host_identity(const RillClient *client);
float rill_client_font_size(const RillClient *client);
const char *rill_client_font_fallback(const RillClient *client, uint32_t index);
float rill_client_padding_x(const RillClient *client);
float rill_client_padding_y(const RillClient *client);
float rill_client_background_opacity(const RillClient *client);
int rill_client_macos_option_as_alt(const RillClient *client);
uint32_t rill_client_background_rgba(const RillClient *client);
uint32_t rill_client_foreground_rgba(const RillClient *client);
uint32_t rill_client_cursor_rgba(const RillClient *client);
/* SPEC-CHROME §4a: derived sidebar fill. Not a theme catalog. */
uint32_t rill_chrome_surface_rgba(uint32_t background);

int rill_client_snapshot(RillClient *client, RillPodGrid *out);

/* --- T-NFR oracle (ADR 0003 D6, D9) ------------------------------------- */

/* Cell-position-specific sentinel support. Scanning the whole grid for a
 * character the shell had already echoed there is what made the old gate
 * unable to fail. */
uint32_t rill_client_cell_codepoint(RillClient *client, uint16_t col, uint16_t row);
int rill_client_cursor(RillClient *client, uint16_t *col, uint16_t *row);

void rill_client_begin_warm_path_audit(RillClient *client);
uint32_t rill_client_end_warm_path_audit(RillClient *client);

#ifdef __cplusplus
}
#endif

#endif
