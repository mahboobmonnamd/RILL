/* One NSWindow. Attaches to a session it did not create.
 *
 * `posix_spawn` here launches rilld and nothing else. The GUI must not create
 * a PTY: no forkpty, openpty, posix_openpt, grantpt, unlockpt, ptsname,
 * login_tty. T-SPAWN checks the *import* tables and runs the same check against
 * a fixture that does create a PTY, so a broken check fails the gate
 * (PRD FR-SPAWN, docs/TEST-CASES.md).
 */

#import <Cocoa/Cocoa.h>
#include "rill_ffi.h"
#import "TerminalView.h"
#include <ApplicationServices/ApplicationServices.h>
#include <spawn.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

extern char **environ;

/* SETSID so the GUI's process group death cannot take the daemon or the shell
 * with it. Removing this flag is T-KILL's required mutation. */
static pid_t spawn_rilld(NSString *rilldPath) {
    if (![[NSFileManager defaultManager] isExecutableFileAtPath:rilldPath]) {
        fprintf(stderr, "Rill: no rilld at %s\n", rilldPath.UTF8String);
        return -1;
    }
    posix_spawnattr_t attr;
    if (posix_spawnattr_init(&attr) != 0) {
        return -1;
    }
    posix_spawnattr_setflags(&attr, POSIX_SPAWN_SETSID);
    const char *path = [rilldPath fileSystemRepresentation];
    char *argv[] = {(char *)path, NULL};
    pid_t pid = 0;
    int rc = posix_spawn(&pid, path, NULL, &attr, argv, environ);
    posix_spawnattr_destroy(&attr);
    if (rc != 0) {
        fprintf(stderr, "Rill: posix_spawn(rilld) failed: %s\n", strerror(rc));
        return -1;
    }
    return pid;
}

/* Poll for readiness instead of sleeping a fixed 150ms. On a loaded machine the
 * old flat usleep opened the app dead with "connect failed" (audit S3-8f). */
static RillClient *connect_with_retry(double timeout_seconds) {
    const useconds_t step_us = 20 * 1000;
    double waited = 0;
    while (waited < timeout_seconds) {
        RillClient *c = rill_client_connect(NULL);
        if (c) {
            return c;
        }
        usleep(step_us);
        waited += (double)step_us / 1e6;
    }
    return NULL;
}

static int parse_nfr_mode(const char *arg, RillNfrMode *out) {
    const char *eq = strchr(arg, '=');
    const char *val = eq ? eq + 1 : "hid";
    if (strcmp(val, "hid") == 0) {
        *out = RillNfrModeHid;
        return 0;
    }
    if (strcmp(val, "app") == 0) {
        *out = RillNfrModeApp;
        return 0;
    }
    return -1;
}

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        BOOL nfr = NO;
        RillNfrMode mode = RillNfrModeHid;
        for (int i = 1; i < argc; i++) {
            if (strncmp(argv[i], "--nfr-key", 9) == 0) {
                nfr = YES;
                if (parse_nfr_mode(argv[i], &mode) != 0) {
                    fprintf(stderr, "Rill: --nfr-key=<hid|app>\n");
                    return 2;
                }
            }
        }

        NSString *exe = [[NSBundle mainBundle] executablePath];
        NSString *dir = [exe stringByDeletingLastPathComponent];
        NSString *rilld = [dir stringByAppendingPathComponent:@"rilld"];

        RillClient *client = rill_client_connect(NULL);
        if (!client) {
            if (spawn_rilld(rilld) < 0) {
                return 1;
            }
            client = connect_with_retry(3.0);
        }
        if (!client) {
            fprintf(stderr, "Rill: %s\n", rill_client_last_error() ?: "connect failed");
            return 1;
        }

        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];

        NSRect rect = NSMakeRect(200, 200, 800, 480);
        NSUInteger style = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                           NSWindowStyleMaskMiniaturizable | NSWindowStyleMaskResizable;
        NSWindow *window = [[NSWindow alloc] initWithContentRect:rect
                                                       styleMask:style
                                                         backing:NSBackingStoreBuffered
                                                           defer:NO];
        window.title = @"RILL";
        TerminalView *view = [[TerminalView alloc] initWithClient:client];
        if (!view) {
            fprintf(stderr, "Rill: renderer failed to initialise\n");
            rill_client_free(client);
            return 1;
        }
        window.contentView = view;
        [window makeFirstResponder:view];
        [window makeKeyAndOrderFront:nil];
        [NSApp activateIgnoringOtherApps:YES];

        if (nfr) {
            /* Do not block on AXIsProcessTrusted, and do not call
             * AXIsProcessTrustedWithOptions(prompt). On current macOS that
             * prompt opens System Settings; enabling Rill there does not
             * make the API true in-process for an adhoc bundle, so a wait
             * is a skip of the gate. HID posts to our own pid. Zero accepted
             * samples is the failure (ADR 0002 D5, 0003 D6). We still do not
             * fall through to --nfr-key=app. */
            Boolean ax = AXIsProcessTrusted();
            if (mode == RillNfrModeHid && !ax) {
                fprintf(stderr,
                        "Rill: AXIsProcessTrusted=false; running hid anyway "
                        "(CGEventPostToPid self).\n");
            }

            RillNfrReport r = [view runNfrKeyWithMode:mode count:1000];

            double budget_ms = r.refresh_hz > 0 ? (1000.0 / r.refresh_hz) : 16.7;
            uint32_t attempted = r.samples + r.discarded;
            double discard_pct = attempted ? (100.0 * r.discarded / attempted) : 100.0;

            printf("T-NFR mode=%s p50=%.3fms p95=%.3fms p99=%.3fms max=%.3fms "
                   "samples=%u discarded=%u (%.2f%%) refresh=%.0fHz budget=%.2fms "
                   "vsync=%d warm_path_violations=%u ax_trusted=%d\n",
                   mode == RillNfrModeHid ? "hid" : "app", r.p50_ms, r.p95_ms, r.p99_ms,
                   r.max_ms, r.samples, r.discarded, discard_pct, r.refresh_hz, budget_ms,
                   r.vsync, r.warm_path_violations, ax ? 1 : 0);

            rill_client_free(client);

            if (!r.ok) {
                fprintf(stderr, "T-NFR: run failed to produce samples\n");
                return 1;
            }
            if (r.samples < 1000) {
                fprintf(stderr, "T-NFR: %u accepted samples, need 1000\n", r.samples);
                return 1;
            }
            if (discard_pct > 2.0) {
                fprintf(stderr,
                        "T-NFR: %.2f%% discards. An unreliable oracle does not get to "
                        "report a p95 (ADR 0003 D6).\n",
                        discard_pct);
                return 1;
            }
            if (r.warm_path_violations != 0) {
                fprintf(stderr, "T-NFR: %u control frames on the warm path\n",
                        r.warm_path_violations);
                return 1;
            }
            if (r.p95_ms >= budget_ms) {
                fprintf(stderr, "T-NFR: p95 %.3fms missed the %.2fms budget\n", r.p95_ms,
                        budget_ms);
                return 1;
            }
            if (mode != RillNfrModeHid) {
                fprintf(stderr,
                        "T-NFR: 'app' mode passed but does NOT close the gate "
                        "(ADR 0003 D7).\n");
                return 1;
            }
            return 0;
        }

        [NSApp run];
        rill_client_free(client);
    }
    return 0;
}
