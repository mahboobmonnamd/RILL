#ifndef RILL_TERMINAL_VIEW_H
#define RILL_TERMINAL_VIEW_H

#import <Cocoa/Cocoa.h>
#import <MetalKit/MetalKit.h>
#include "rill_ffi.h"

/* Injection path for T-NFR (ADR 0003 D7). Only HID closes the gate. */
typedef enum {
    RillNfrModeApp = 0, /* NSEvent -> [NSApp sendEvent:]; CI diagnostic only */
    RillNfrModeHid = 1  /* CGEventPost -> window server; gate-closing         */
} RillNfrMode;

typedef struct {
    double p50_ms;
    double p95_ms;
    double p99_ms;
    double max_ms;
    uint32_t samples;
    uint32_t discarded;
    uint32_t warm_path_violations;
    double refresh_hz;
    int vsync;
    int mode;
    int ok;
} RillNfrReport;

@interface TerminalView : MTKView <MTKViewDelegate, NSTextInputClient, NSWindowDelegate>

- (instancetype)initWithClient:(RillClient *)client;

/* Blocks on the run loop until `count` samples are accepted or the run fails.
 * Measures NSEvent.timestamp -> CAMetalDrawable.presentedTime, with a
 * cell-position-specific sentinel (ADR 0003 D5, D6). */
- (RillNfrReport)runNfrKeyWithMode:(RillNfrMode)mode count:(uint32_t)count;

@end

#endif
