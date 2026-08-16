#ifndef TERMINAL_VIEW_H
#define TERMINAL_VIEW_H

#import <Cocoa/Cocoa.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#include "rill_ffi.h"

@interface TerminalView : NSView
- (instancetype)initWithClient:(RillClient *)client;
- (void)pump;
@end

#endif
