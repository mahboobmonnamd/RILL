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
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <unistd.h>

extern char **environ;

/* `open App.app` can apply Info.plist LSEnvironment without HOME/SHELL.
 * rilld then uses /var/empty and the GUI exits before any window. */
static void rill_fill_launch_env(void) {
    if (!getenv("HOME") || getenv("HOME")[0] == '\0') {
        NSString *home = NSHomeDirectory();
        if (home.length) {
            setenv("HOME", home.fileSystemRepresentation, 1);
        }
    }
    if (!getenv("SHELL") || getenv("SHELL")[0] == '\0') {
        setenv("SHELL", "/bin/zsh", 0);
    }
}

static NSString *rill_runtime_dir(void) {
    const char *home = getenv("HOME");
    if (!home || !home[0]) {
        return nil;
    }
    return [NSString stringWithFormat:@"%s/Library/Application Support/RILL/run", home];
}

static NSString *rill_rilld_log_path(void) {
    NSString *dir = rill_runtime_dir();
    if (!dir) {
        return nil;
    }
    [[NSFileManager defaultManager] createDirectoryAtPath:dir
                              withIntermediateDirectories:YES
                                               attributes:@{NSFilePosixPermissions : @0700}
                                                    error:NULL];
    return [dir stringByAppendingPathComponent:@"rilld.log"];
}

static void rill_unlink_stale_runtime_sockets(void) {
    if (getenv("RILL_SOCKET")) {
        return;
    }
    NSString *dir = rill_runtime_dir();
    if (!dir) {
        return;
    }
    NSArray<NSString *> *names = @[
        @"attach.sock",
        @"attach.sock.identity",
        @"attach.sock.nav",
        @"attach.sock.worker",
        @"attach.sock.worker.identity",
        @"attach.sock.worker.nav",
    ];
    NSFileManager *fm = [NSFileManager defaultManager];
    for (NSString *name in names) {
        NSString *path = [dir stringByAppendingPathComponent:name];
        if (![fm fileExistsAtPath:path]) {
            continue;
        }
        int fd = socket(AF_UNIX, SOCK_STREAM, 0);
        if (fd >= 0) {
            struct sockaddr_un addr;
            memset(&addr, 0, sizeof(addr));
            addr.sun_family = AF_UNIX;
            const char *cpath = path.fileSystemRepresentation;
            if (strlen(cpath) < sizeof(addr.sun_path)) {
                strncpy(addr.sun_path, cpath, sizeof(addr.sun_path) - 1);
                if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) == 0) {
                    close(fd);
                    continue;
                }
            }
            close(fd);
        }
        [fm removeItemAtPath:path error:NULL];
    }
}

static NSString *rill_read_log_tail(NSString *path) {
    if (!path) {
        return @"";
    }
    NSData *data = [NSData dataWithContentsOfFile:path];
    if (!data.length) {
        return @"";
    }
    NSUInteger keep = MIN((NSUInteger)1200, data.length);
    NSData *slice = [data subdataWithRange:NSMakeRange(data.length - keep, keep)];
    NSString *text = [[NSString alloc] initWithData:slice encoding:NSUTF8StringEncoding];
    return text ?: @"";
}

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
- (void)toggleNavFromMenu:(id)sender;
- (void)toggleInspectorFromMenu:(id)sender;
- (void)newTabFromMenu:(id)sender;
- (void)selectTabFromMenu:(id)sender;
- (IBAction)closeWindowForced:(id)sender;
@end
@implementation RillAppDelegate
- (void)toggleNavFromMenu:(id)sender {
    (void)sender;
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "always_forward_chord") == 0) {
        return;
    }
    [[NSNotificationCenter defaultCenter] postNotificationName:@"RillToggleNav" object:self];
}
- (void)toggleInspectorFromMenu:(id)sender {
    (void)sender;
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "always_forward_chord") == 0) {
        return;
    }
    [[NSNotificationCenter defaultCenter] postNotificationName:@"RillToggleInspector"
                                                        object:self];
}
- (void)newTabFromMenu:(id)sender {
    (void)sender;
    [[NSNotificationCenter defaultCenter] postNotificationName:@"RillNewTab" object:self];
}
- (void)selectTabFromMenu:(id)sender {
    NSInteger n = [(NSMenuItem *)sender tag];
    [[NSNotificationCenter defaultCenter] postNotificationName:@"RillSelectTab"
                                                        object:self
                                                      userInfo:@{@"i" : @(n)}];
}
- (IBAction)closeWindowForced:(id)sender {
    [self.window performClose:sender];
}
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

- (IBAction)closeMainWindow:(id)sender {
    (void)sender;
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "always_close_window") == 0) {
        [self.window performClose:sender];
        return;
    }
    if ([self.window.contentView isKindOfClass:[TerminalView class]]) {
        [self.window performClose:sender];
        return;
    }
    [[NSNotificationCenter defaultCenter] postNotificationName:@"RillCloseInnermost" object:self];
}
@end

static void rill_install_menus(id appDelegate) {
    NSMenu *menubar = [[NSMenu alloc] init];

    NSMenuItem *appItem = [[NSMenuItem alloc] init];
    NSMenu *appMenu = [[NSMenu alloc] initWithTitle:@"Rill"];
    NSMenuItem *quit = [[NSMenuItem alloc] initWithTitle:@"Quit Rill"
                                                  action:@selector(terminate:)
                                           keyEquivalent:@"q"];
    quit.target = NSApp;
    const char *mut = getenv("RILL_MUTATE");
    if (!(mut && strcmp(mut, "skip_app_quit_menu") == 0)) {
        [appMenu addItem:quit];
    }
    appItem.submenu = appMenu;
    [menubar addItem:appItem];

    NSMenuItem *fileItem = [[NSMenuItem alloc] init];
    NSMenu *file = [[NSMenu alloc] initWithTitle:@"File"];
    NSMenuItem *close = [[NSMenuItem alloc] initWithTitle:@"Close Tab"
                                                   action:@selector(closeMainWindow:)
                                            keyEquivalent:@"w"];
    close.target = appDelegate;
    [file addItem:close];
    NSMenuItem *newTab = [[NSMenuItem alloc] initWithTitle:@"New Tab"
                                                    action:@selector(newTabFromMenu:)
                                             keyEquivalent:@"t"];
    newTab.target = appDelegate;
    [file addItem:newTab];
    NSMenuItem *closeWin = [[NSMenuItem alloc] initWithTitle:@"Close Window"
                                                      action:@selector(closeWindowForced:)
                                               keyEquivalent:@"W"];
    closeWin.target = appDelegate;
    [file addItem:closeWin];
    fileItem.submenu = file;
    [menubar addItem:fileItem];

    NSMenuItem *editItem = [[NSMenuItem alloc] init];
    NSMenu *edit = [[NSMenu alloc] initWithTitle:@"Edit"];
    NSMenuItem *pasteItem = [[NSMenuItem alloc] initWithTitle:@"Paste"
                                                       action:@selector(paste:)
                                                keyEquivalent:@"v"];
    [edit addItem:pasteItem];
    editItem.submenu = edit;
    [menubar addItem:editItem];

    NSMenuItem *viewItem = [[NSMenuItem alloc] init];
    NSMenu *view = [[NSMenu alloc] initWithTitle:@"View"];
    NSMenuItem *optCmd = [[NSMenuItem alloc] initWithTitle:@"Toggle Workspaces"
                                                    action:@selector(toggleNavFromMenu:)
                                             keyEquivalent:@"1"];
    optCmd.keyEquivalentModifierMask = NSEventModifierFlagCommand | NSEventModifierFlagOption;
    optCmd.target = appDelegate;
    [view addItem:optCmd];
    NSMenuItem *ctrlCmd = [[NSMenuItem alloc] initWithTitle:@"Toggle Inspector"
                                                     action:@selector(toggleInspectorFromMenu:)
                                              keyEquivalent:@"1"];
    ctrlCmd.keyEquivalentModifierMask = NSEventModifierFlagCommand | NSEventModifierFlagControl;
    ctrlCmd.target = appDelegate;
    [view addItem:ctrlCmd];
    viewItem.submenu = view;
    [menubar addItem:viewItem];

    NSMenuItem *winItem = [[NSMenuItem alloc] init];
    NSMenu *win = [[NSMenu alloc] initWithTitle:@"Window"];
    if (!(mut && strcmp(mut, "skip_tab_index_keys") == 0)) {
        for (NSInteger i = 1; i <= 9; i++) {
            NSString *title = [NSString stringWithFormat:@"Show Tab %ld", (long)i];
            NSString *key = [NSString stringWithFormat:@"%ld", (long)i];
            NSMenuItem *tabKey = [[NSMenuItem alloc] initWithTitle:title
                                                            action:@selector(selectTabFromMenu:)
                                                     keyEquivalent:key];
            tabKey.tag = i - 1;
            tabKey.target = appDelegate;
            [win addItem:tabKey];
        }
    }
    winItem.submenu = win;
    [menubar addItem:winItem];

    NSApp.mainMenu = menubar;
}

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
    posix_spawn_file_actions_t actions;
    BOOL have_actions = posix_spawn_file_actions_init(&actions) == 0;
    int logfd = -1;
    NSString *logPath = rill_rilld_log_path();
    if (have_actions && logPath) {
        logfd = open(logPath.fileSystemRepresentation, O_WRONLY | O_CREAT | O_APPEND, 0600);
        if (logfd >= 0) {
            posix_spawn_file_actions_adddup2(&actions, logfd, STDERR_FILENO);
            posix_spawn_file_actions_adddup2(&actions, logfd, STDOUT_FILENO);
        }
    }
    const char *path = [rilldPath fileSystemRepresentation];
    char *argv[] = {(char *)path, NULL};
    pid_t pid = 0;
    int rc = posix_spawn(&pid, path, have_actions ? &actions : NULL, &attr, argv, environ);
    if (have_actions) {
        posix_spawn_file_actions_destroy(&actions);
    }
    if (logfd >= 0) {
        close(logfd);
    }
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
static pid_t ensure_supervised_runtime(NSString *rilldPath) {
    const char *mut = getenv("RILL_MUTATE");
    BOOL unregistered = mut && strcmp(mut, "posix_spawn_unregistered") == 0;
    if (unregistered) {
        return spawn_rilld(rilldPath);
    }
    if (getenv("RILL_SOCKET") || getenv("RILL_DEV_DIRECT_RILLD")) {
        rill_unlink_stale_runtime_sockets();
        return spawn_rilld(rilldPath);
    }
    if (@available(macOS 13.0, *)) {
        SMAppService *agent = [SMAppService agentServiceWithPlistName:@"dev.rill.rilld.plist"];
        NSError *err = nil;
        if (agent.status != SMAppServiceStatusEnabled) {
            if (![agent registerAndReturnError:&err]) {
                fprintf(stderr, "Rill: runtime service not registered\n");
                return -1;
            }
        }
        return 0;
    }
    fprintf(stderr, "Rill: Service Management required\n");
    return -1;
}

/* Poll for readiness instead of sleeping a fixed 150ms. On a loaded machine the
 * old flat usleep opened the app dead with "connect failed" (audit S3-8f). */
static RillClient *connect_with_retry(double timeout_seconds, pid_t daemon_pid) {
    const useconds_t step_us = 20 * 1000;
    double waited = 0;
    while (waited < timeout_seconds) {
        RillClient *c = rill_client_connect(NULL);
        if (c) {
            return c;
        }
        if (daemon_pid > 0) {
            int st = 0;
            pid_t w = waitpid(daemon_pid, &st, WNOHANG);
            if (w == daemon_pid) {
                return NULL;
            }
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
        rill_fill_launch_env();
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

        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];

        RillClient *client = rill_client_connect(NULL);
        if (!client) {
            pid_t daemon = ensure_supervised_runtime(rilld);
            if (daemon < 0) {
                return 1;
            }
            client = connect_with_retry(8.0, daemon);
        }
        if (!client) {
            const char *why = rill_client_last_error() ?: "connect failed";
            NSString *log = rill_read_log_tail(rill_rilld_log_path());
            fprintf(stderr, "Rill: %s\n", why);
            NSLog(@"Rill: %s", why);
            if (!nfr && !getenv("RILL_SOCKET") && !getenv("RILL_TEST_HEARTBEAT")) {
                NSAlert *alert = [[NSAlert alloc] init];
                alert.messageText = @"Rill could not start";
                NSMutableString *info = [NSMutableString stringWithFormat:@"%s", why];
                if (log.length) {
                    [info appendFormat:@"\n\n%@", log];
                } else {
                    [info appendString:@"\n\nNo rilld.log yet. From the repo run: make run"];
                }
                alert.informativeText = info;
                [alert runModal];
            }
            return 1;
        }
        RillAppDelegate *appDelegate = [RillAppDelegate new];
        [NSApp setDelegate:appDelegate];
        rill_install_menus(appDelegate);

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
                                                    workspaceId:rill_client_workspace_id(client)
                                                      topInset:rill_client_padding_y(client)];
            window.contentViewController = chrome;
        }
        window.delegate = view;
        [window makeKeyAndOrderFront:nil];
        [window makeFirstResponder:view];
        [NSApp activateIgnoringOtherApps:YES];

        if (getenv("RILL_TEST_CLOSE_WINDOW")) {
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.5 * NSEC_PER_SEC)),
                           dispatch_get_main_queue(), ^{
                             [window performClose:nil];
                             [view writeTestHeartbeat];
                           });
        }

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
