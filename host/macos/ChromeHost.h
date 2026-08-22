#ifndef RILL_CHROME_HOST_H
#define RILL_CHROME_HOST_H

#import <Cocoa/Cocoa.h>
#include <stdint.h>
@class TerminalView;

/* Three-column chrome around Chip 1 (ADR 0018, SPEC-CHROME, ADR 0056).
 * New Tab is kernel spawn_leaf, not a second VT. Not live agents. */
@interface RillChromeController : NSViewController
- (instancetype)initWithTerminal:(TerminalView *)terminal
                      background:(uint32_t)bg
                      foreground:(uint32_t)fg
                            host:(NSString *)host
                      workspaceId:(uint64_t)workspaceId
                        topInset:(CGFloat)topInset;
- (void)toggleNav;
- (void)toggleInspector;
- (BOOL)sidebarsHidden;
@end

#endif
