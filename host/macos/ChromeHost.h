#ifndef RILL_CHROME_HOST_H
#define RILL_CHROME_HOST_H

#import <Cocoa/Cocoa.h>
#include <stdint.h>
@class TerminalView;

/* Three-column chrome around one Chip 1 leaf (ADR 0018, SPEC-CHROME, ADR 0054).
 * Not a second VT. Not tabs, nested splits, or agents. */
@interface RillChromeController : NSViewController
- (instancetype)initWithTerminal:(TerminalView *)terminal
                      background:(uint32_t)bg
                      foreground:(uint32_t)fg
                            host:(NSString *)host
                        topInset:(CGFloat)topInset;
@end

#endif
