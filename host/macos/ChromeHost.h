#ifndef RILL_CHROME_HOST_H
#define RILL_CHROME_HOST_H

#import <Cocoa/Cocoa.h>
@class TerminalView;

/* Three-column chrome around one Chip 0 leaf (ADR 0018, SPEC-CHROME).
 * Not a second VT. Not tabs, nested splits, or agents. */
@interface RillChromeController : NSViewController
- (instancetype)initWithTerminal:(TerminalView *)terminal;
@end

#endif
