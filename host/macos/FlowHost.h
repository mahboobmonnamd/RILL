#ifndef RILL_FLOW_HOST_H
#define RILL_FLOW_HOST_H

#import <Cocoa/Cocoa.h>
#include "rill_ffi.h"

@interface RillFlowHost : NSView
@property (nonatomic, assign) RillClient *client;
@property (nonatomic, assign) BOOL rawFallback;
- (instancetype)initWithFrame:(NSRect)frame
                       client:(RillClient *)client
                   fontFamily:(NSString *)family
                     fontSize:(CGFloat)size
                           bg:(uint32_t)bg
                           fg:(uint32_t)fg;
- (void)reloadFromClient;
- (BOOL)composerVisible;
@end

#endif
