#import <Cocoa/Cocoa.h>
#include "rill_ffi.h"
#import "TerminalView.h"
#include <spawn.h>
#include <string.h>
#include <unistd.h>

extern char **environ;

static void spawn_rilld(NSString *rilldPath) {
    if (![[NSFileManager defaultManager] fileExistsAtPath:rilldPath]) {
        return;
    }
    posix_spawnattr_t attr;
    posix_spawnattr_init(&attr);
    posix_spawnattr_setflags(&attr, POSIX_SPAWN_SETSID);
    const char *path = [rilldPath fileSystemRepresentation];
    char *argv[] = {(char *)path, NULL};
    pid_t pid = 0;
    (void)posix_spawn(&pid, path, NULL, &attr, argv, environ);
    posix_spawnattr_destroy(&attr);
    usleep(150 * 1000);
}

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        BOOL nfr = NO;
        for (int i = 1; i < argc; i++) {
            if (strcmp(argv[i], "--nfr-key") == 0) {
                nfr = YES;
            }
        }

        NSString *exe = [[NSBundle mainBundle] executablePath];
        NSString *dir = [exe stringByDeletingLastPathComponent];
        NSString *rilld = [dir stringByAppendingPathComponent:@"rilld"];
        RillClient *client = rill_client_connect(NULL);
        if (!client) {
            spawn_rilld(rilld);
            client = rill_client_connect(NULL);
        }
        if (!client) {
            fprintf(stderr, "Rill: %s\n", rill_client_last_error() ?: "connect failed");
            return 1;
        }

        if (nfr) {
            double p95 = 0;
            int rpc = 0;
            int batt = 0;
            int rc = rill_client_nfr_key(client, 1000, &p95, &rpc, &batt);
            printf("T-NFR p95=%.3fms control_rpc=%d battery=%d rc=%d\n", p95, rpc, batt, rc);
            rill_client_free(client);
            if (rc != 0 || rpc || p95 >= 16.7) {
                return 1;
            }
            return 0;
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
        window.contentView = view;
        [window makeKeyAndOrderFront:nil];
        [NSApp activateIgnoringOtherApps:YES];
        [NSApp run];
        rill_client_free(client);
    }
    return 0;
}
