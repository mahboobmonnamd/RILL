/* One NSWindow. Attaches to a session it did not create.
 *
 * `posix_spawn` here launches rilld and nothing else. The GUI must not create
 * a PTY: no forkpty, openpty, posix_openpt, grantpt, unlockpt, ptsname,
 * login_tty. T-SPAWN checks the *import* tables and runs the same check against
 * a fixture that does create a PTY, so a broken check fails the gate
 * (PRD FR-SPAWN, docs/TEST-CASES.md).
 */

#import <Cocoa/Cocoa.h>
#import <ServiceManagement/ServiceManagement.h>
#include "rill_ffi.h"
#import "ChromeHost.h"
#import "TerminalView.h"
#include <ApplicationServices/ApplicationServices.h>
#include <signal.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <unistd.h>

extern char **environ;

@interface RillWindow : NSWindow
@end
@implementation RillWindow
- (BOOL)canBecomeKeyWindow {
    return YES;
}
- (BOOL)canBecomeMainWindow {
    return YES;
}
@end

@interface RillAppDelegate : NSObject <NSApplicationDelegate>
@property (nonatomic, strong) NSWindow *window;
@end
@implementation RillAppDelegate
- (void)restoreWindowForDockReopen {
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "skip_dock_reopen") == 0) {
        return;
    }
    NSWindow *w = self.window;
    if (!w) {
        return;
    }
    [NSApp unhide:nil];
    if (w.miniaturized) {
        [w deminiaturize:nil];
    }
    [w makeKeyAndOrderFront:nil];
    [NSApp activateIgnoringOtherApps:YES];
}

- (BOOL)applicationShouldHandleReopen:(NSApplication *)sender hasVisibleWindows:(BOOL)flag {
    (void)sender;
    (void)flag;
    [self restoreWindowForDockReopen];
    return YES;
}

- (void)applicationDidBecomeActive:(NSNotification *)notification {
    (void)notification;
    /* T-DOCK-REOPEN hides then sends reopen; do not undo the hide here. */
    if (getenv("RILL_TEST_DOCK_REOPEN")) {
        return;
    }
    NSWindow *w = self.window;
    if (w && (!w.isVisible || !w.isOnActiveSpace)) {
        [self restoreWindowForDockReopen];
    }
}

- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender {
    (void)sender;
    return NO;
}
@end

/* Apple: direct-to-display needs toggleFullScreen: on a titled window, not a
 * borderless cover of screen.frame. Default launch is windowed (ADR 0017).
 * T-NFR and T-FS-EXIT still enter a Space. Pump until the Space is actually
 * entered. */
static BOOL wait_until_fullscreen(NSWindow *window, NSTimeInterval timeout) {
    NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:timeout];
    while ((window.styleMask & NSWindowStyleMaskFullScreen) == 0 &&
           [deadline timeIntervalSinceNow] > 0) {
        NSEvent *e = [NSApp nextEventMatchingMask:NSEventMaskAny
                                        untilDate:[NSDate dateWithTimeIntervalSinceNow:0.01]
                                           inMode:NSDefaultRunLoopMode
                                          dequeue:YES];
        if (e) {
            [NSApp sendEvent:e];
        }
    }
    return (window.styleMask & NSWindowStyleMaskFullScreen) != 0;
}

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
    short flags = POSIX_SPAWN_SETSID;
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "drop_POSIX_SPAWN_SETSID") == 0) {
        flags = 0;
    }
    posix_spawnattr_setflags(&attr, flags);
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

/* Production: per-user agent via SMAppService. Unique RILL_SOCKET is the
 * bounded development/test path (SPEC-RUNTIME-SUPERVISION §1). Mutation
 * posix_spawn_unregistered skips registration and launches an unregistered
 * daemon (T-RUNTIME-GUI-INDEPENDENT). */
static BOOL ensure_supervised_runtime(NSString *rilldPath) {
    const char *mut = getenv("RILL_MUTATE");
    BOOL unregistered = mut && strcmp(mut, "posix_spawn_unregistered") == 0;
    if (unregistered) {
        return spawn_rilld(rilldPath) >= 0;
    }
    if (getenv("RILL_SOCKET") || getenv("RILL_DEV_DIRECT_RILLD")) {
        return spawn_rilld(rilldPath) >= 0;
    }
    if (@available(macOS 13.0, *)) {
        SMAppService *agent = [SMAppService agentServiceWithPlistName:@"dev.rill.rilld.plist"];
        NSError *err = nil;
        if (agent.status != SMAppServiceStatusEnabled) {
            if (![agent registerAndReturnError:&err]) {
                fprintf(stderr, "Rill: runtime service not registered\n");
                return NO;
            }
        }
        return YES;
    }
    fprintf(stderr, "Rill: Service Management required\n");
    return NO;
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
            if (!ensure_supervised_runtime(rilld)) {
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
        RillAppDelegate *appDelegate = [RillAppDelegate new];
        [NSApp setDelegate:appDelegate];

        NSRect frame = NSMakeRect(80, 80, 1100, 680);
        RillWindow *window = [[RillWindow alloc]
            initWithContentRect:frame
                      styleMask:(NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                                 NSWindowStyleMaskMiniaturizable | NSWindowStyleMaskResizable)
                        backing:NSBackingStoreBuffered
                          defer:NO];
        window.opaque = YES;
        window.releasedWhenClosed = NO;
        uint32_t bg = rill_client_background_rgba(client);
        const char *host_identity = rill_client_host_identity(client);
        if (!host_identity) {
            fprintf(stderr, "Rill: missing cold kernel host identity\n");
            rill_client_free(client);
            return 1;
        }
        NSString *host = [NSString stringWithUTF8String:host_identity];
        if (!host) {
            fprintf(stderr, "Rill: invalid cold kernel host identity\n");
            rill_client_free(client);
            return 1;
        }
        const char *identity_mutation = getenv("RILL_MUTATE");
        if (identity_mutation && strcmp(identity_mutation, "host_indicator_from_home") == 0) {
            const char *home = getenv("HOME");
            host = home ? [NSString stringWithUTF8String:home] : @"Rill";
        }
        window.backgroundColor = [NSColor colorWithRed:((bg >> 24) & 0xff) / 255.0
                                                 green:((bg >> 16) & 0xff) / 255.0
                                                  blue:((bg >> 8) & 0xff) / 255.0
                                                 alpha:1.0];
        window.title = @"Rill";
        window.collectionBehavior = NSWindowCollectionBehaviorFullScreenPrimary;
        appDelegate.window = window;
        TerminalView *view = [[TerminalView alloc] initWithClient:client];
        if (!view) {
            fprintf(stderr, "Rill: renderer failed to initialise\n");
            rill_client_free(client);
            return 1;
        }

        /* T-NFR closer is TerminalView as contentView (ADR 0009 / 0018 D2).
         * Mutation no_chrome is T-SPLIT's required invert. */
        BOOL skip_chrome = nfr;
        const char *chrome_mut = getenv("RILL_MUTATE");
        if (chrome_mut && strcmp(chrome_mut, "no_chrome") == 0) {
            skip_chrome = YES;
        }
        if (skip_chrome) {
            window.contentView = view;
        } else {
            RillChromeController *chrome =
                [[RillChromeController alloc] initWithTerminal:view
                                                    background:rill_client_background_rgba(client)
                                                    foreground:rill_client_foreground_rgba(client)
                                                          host:host
                                                      topInset:rill_client_padding_y(client)];
            window.contentViewController = chrome;
        }
        window.delegate = view;
        [window makeKeyAndOrderFront:nil];
        [window makeFirstResponder:view];
        [NSApp activateIgnoringOtherApps:YES];

        const char *mut = getenv("RILL_MUTATE");
        BOOL always_fs = mut && strcmp(mut, "always_toggle_fullscreen") == 0;
        BOOL test_leave = getenv("RILL_TEST_EXIT_FULLSCREEN") != NULL;
        BOOL enter_fs = nfr || always_fs || test_leave;
        if (enter_fs) {
            [window toggleFullScreen:nil];
            if (!wait_until_fullscreen(window, 5.0)) {
                fprintf(stderr, "Rill: toggleFullScreen did not enter a Space\n");
            }
        } else {
            if (mut && strcmp(mut, "window_alpha_from_opacity") == 0) {
                float opacity = rill_client_background_opacity(client);
                if (opacity < 0.999f) {
                    window.alphaValue = (CGFloat)opacity;
                }
            }
        }
        if (test_leave) {
            /* Same call as the green traffic-light (ADR 0016). Enter already
             * happened above; this leaves. */
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.6 * NSEC_PER_SEC)),
                           dispatch_get_main_queue(), ^{
                               [window toggleFullScreen:nil];
                           });
        }
        if (getenv("RILL_TEST_MOBILE_BACKGROUND")) {
            int attach_fd = rill_client_socket_fd(client);
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.25 * NSEC_PER_SEC)),
                           dispatch_get_main_queue(), ^{
                               const char *bg_mut = getenv("RILL_MUTATE");
                               if (bg_mut && strcmp(bg_mut, "background_terminates") == 0) {
                                   const char *pf = getenv("RILL_TEST_PIDFILE");
                                   FILE *f = pf ? fopen(pf, "r") : NULL;
                                   unsigned child = 0;
                                   if (f && fscanf(f, "%u", &child) == 1 && child > 1) {
                                       kill((pid_t)child, SIGKILL);
                                   }
                                   if (f) {
                                       fclose(f);
                                   }
                               }
                               if (attach_fd >= 0) {
                                   shutdown(attach_fd, SHUT_RDWR);
                               }
                           });
        }
        if (getenv("RILL_TEST_DOCK_REOPEN")) {
            /* Same selector Dock sends (ADR 0019). Hide first so the oracle
             * can observe not-visible, then reopen. */
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.6 * NSEC_PER_SEC)),
                           dispatch_get_main_queue(), ^{
                               [window orderOut:nil];
                               [view writeTestHeartbeat];
                           });
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1.2 * NSEC_PER_SEC)),
                           dispatch_get_main_queue(), ^{
                               id del = [NSApp delegate];
                               if ([del respondsToSelector:@selector(applicationShouldHandleReopen:
                                                                     hasVisibleWindows:)]) {
                                   [del applicationShouldHandleReopen:NSApp hasVisibleWindows:NO];
                               }
                               [view writeTestHeartbeat];
                           });
        }

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
