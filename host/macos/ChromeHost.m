/* M2 chrome: nav | Chip 1 | inspector. AppKit only. The center subview is
 * TerminalView (Metal). Sidebars do not paint PTY bytes (ADR 0018). */

#import "ChromeHost.h"
#import "TerminalView.h"
#include "rill_ffi.h"
#include <stdlib.h>
#include <string.h>

static NSColor *RillRgbaColor(uint32_t rgba) {
    return [NSColor colorWithSRGBRed:((rgba >> 24) & 0xff) / 255.0
                               green:((rgba >> 16) & 0xff) / 255.0
                                blue:((rgba >> 8) & 0xff) / 255.0
                               alpha:1.0];
}

static NSColor *RillMutedBetween(uint32_t bg, uint32_t fg) {
    uint32_t mix = 0;
    for (int shift = 24; shift >= 8; shift -= 8) {
        unsigned b = (bg >> shift) & 0xffu;
        unsigned f = (fg >> shift) & 0xffu;
        unsigned m = (b * 3u + f * 2u) / 5u;
        mix |= m << shift;
    }
    mix |= 0xffu;
    return RillRgbaColor(mix);
}
static const CGFloat kRillNavWidth = 200.0;
static const CGFloat kRillInspectorWidth = 180.0;
static const CGFloat kRillNavMin = 160.0;
static const CGFloat kRillInspectorMin = 140.0;
static const CGFloat kRillCenterMin = 320.0;

@interface RillChromePane : NSView
@property (nonatomic, weak) TerminalView *terminal;
@property (nonatomic, strong) NSView *heading;
@property (nonatomic, strong) NSMutableArray<NSView *> *rows;
@property (nonatomic, assign) CGFloat topInset;
@property (nonatomic, assign) BOOL freezeFrames;
@end

@implementation RillChromePane
- (BOOL)isFlipped {
    return !self.freezeFrames;
}
- (BOOL)acceptsFirstResponder {
    return NO;
}
- (void)mouseDown:(NSEvent *)event {
    (void)event;
    if (self.terminal) {
        [self.window makeFirstResponder:self.terminal];
    }
}
- (void)layout {
    [super layout];
    if (self.freezeFrames) {
        return;
    }
    CGFloat w = self.bounds.size.width;
    CGFloat y = self.topInset;
    if (self.heading) {
        self.heading.frame = NSMakeRect(14, y, MAX(8.0, w - 28), 18);
        y += 22;
    }
    for (NSView *row in self.rows) {
        row.frame = NSMakeRect(8, y, MAX(8.0, w - 16), 28);
        y += 32;
    }
}
@end

@interface RillChromeController () <NSSplitViewDelegate>
@property (nonatomic, strong) TerminalView *terminal;
@property (nonatomic, assign) BOOL positioned;
@property (nonatomic, assign) uint32_t bgRgba;
@property (nonatomic, assign) uint32_t fgRgba;
@property (nonatomic, assign) uint32_t paneRgba;
@property (nonatomic, assign) CGFloat topInset;
@property (nonatomic, assign) BOOL freezeY;
@property (nonatomic, copy) NSString *hostIdentity;
@property (nonatomic, assign) uint64_t workspaceId;
@property (nonatomic, assign) BOOL navHidden;
@property (nonatomic, assign) BOOL inspectorHidden;
@property (nonatomic, assign) BOOL hideDetaches;
@end

@implementation RillChromeController

- (instancetype)initWithTerminal:(TerminalView *)terminal
                      background:(uint32_t)bg
                      foreground:(uint32_t)fg
                            host:(NSString *)host
                      workspaceId:(uint64_t)workspaceId
                        topInset:(CGFloat)topInset {
    self = [super initWithNibName:nil bundle:nil];
    if (self) {
        _terminal = terminal;
        _topInset = topInset;
        _hostIdentity = [host copy];
        _workspaceId = workspaceId;
        const char *mut = getenv("RILL_MUTATE");
        _freezeY = mut && strcmp(mut, "hardcoded_chrome_y") == 0;
        _hideDetaches = mut && strcmp(mut, "hide_sidebar_detaches") == 0;
        if (mut && strcmp(mut, "chrome_invents_workspace_row") == 0) {
            _workspaceId = 0;
        }
        if (mut && strcmp(mut, "hardcoded_chrome_gray") == 0) {
            _bgRgba = 0x1e1e1eff;
            _fgRgba = 0xdbdbdbff;
            _paneRgba = _bgRgba;
        } else {
            _bgRgba = bg;
            _fgRgba = fg;
            _paneRgba = rill_chrome_surface_rgba(bg);
        }
    }
    return self;
}

- (void)loadView {
    NSSplitView *split = [[NSSplitView alloc] initWithFrame:NSMakeRect(0, 0, 1100, 680)];
    split.vertical = YES;
    split.dividerStyle = NSSplitViewDividerStyleThin;
    split.delegate = self;
    split.accessibilityIdentifier = @"chrome-split";
    split.wantsLayer = YES;
    split.layer.backgroundColor = RillRgbaColor(self.paneRgba).CGColor;

    NSString *homeName = NSHomeDirectory().lastPathComponent ?: @"Rill";
    RillChromePane *left = [self navPaneNamed:homeName];
    left.terminal = self.terminal;
    left.accessibilityIdentifier = @"chrome-left";

    self.terminal.accessibilityIdentifier = @"chrome-center";

    RillChromePane *right = [self inspectorPaneNamed:self.hostIdentity];
    right.terminal = self.terminal;
    right.accessibilityIdentifier = @"chrome-right";

    [split addSubview:left];
    [split addSubview:self.terminal];
    [split addSubview:right];
    [split setHoldingPriority:NSLayoutPriorityDefaultHigh forSubviewAtIndex:0];
    [split setHoldingPriority:NSLayoutPriorityDefaultLow forSubviewAtIndex:1];
    [split setHoldingPriority:NSLayoutPriorityDefaultHigh forSubviewAtIndex:2];
    self.view = split;
    [[NSNotificationCenter defaultCenter] addObserver:self
                                             selector:@selector(toggleNav)
                                                 name:@"RillToggleNav"
                                               object:nil];
    [[NSNotificationCenter defaultCenter] addObserver:self
                                             selector:@selector(toggleInspector)
                                                 name:@"RillToggleInspector"
                                               object:nil];
    [[NSNotificationCenter defaultCenter] addObserver:self
                                             selector:@selector(toggleNav)
                                                 name:@"RillToggleSidebars"
                                               object:nil];
}

- (void)applySplitPositions {
    NSSplitView *split = (NSSplitView *)self.view;
    if (![split isKindOfClass:[NSSplitView class]] || split.subviews.count < 3) {
        return;
    }
    NSView *left = split.subviews[0];
    NSView *right = split.subviews[2];
    left.hidden = _navHidden;
    right.hidden = _inspectorHidden;
    CGFloat w = split.bounds.size.width;
    CGFloat leftW = _navHidden ? 0 : kRillNavWidth;
    CGFloat rightW = _inspectorHidden ? 0 : kRillInspectorWidth;
    [split setPosition:leftW ofDividerAtIndex:0];
    [split setPosition:(w - rightW) ofDividerAtIndex:1];
    [split layoutSubtreeIfNeeded];
}

- (void)viewDidLayout {
    [super viewDidLayout];
    if (self.positioned) {
        return;
    }
    NSSplitView *split = (NSSplitView *)self.view;
    CGFloat w = split.bounds.size.width;
    if (w < kRillCenterMin) {
        return;
    }
    [self applySplitPositions];
    self.positioned = YES;
}

- (CGFloat)splitView:(NSSplitView *)splitView
    constrainMinCoordinate:(CGFloat)proposed
              ofSubviewAt:(NSInteger)dividerIndex {
    CGFloat w = splitView.bounds.size.width;
    if (dividerIndex == 0) {
        return _navHidden ? 0 : MAX(proposed, kRillNavMin);
    }
    CGFloat leftW = _navHidden ? 0 : kRillNavMin;
    if (_inspectorHidden) {
        return w;
    }
    return MAX(proposed, leftW + kRillCenterMin);
}

- (CGFloat)splitView:(NSSplitView *)splitView
    constrainMaxCoordinate:(CGFloat)proposed
              ofSubviewAt:(NSInteger)dividerIndex {
    CGFloat w = splitView.bounds.size.width;
    if (dividerIndex == 0) {
        if (_navHidden) {
            return 0;
        }
        CGFloat rightW = _inspectorHidden ? 0 : kRillInspectorMin;
        return MIN(proposed, w - rightW - kRillCenterMin);
    }
    if (_inspectorHidden) {
        return w;
    }
    return MIN(proposed, w - kRillInspectorMin);
}

- (BOOL)splitView:(NSSplitView *)splitView canCollapseSubview:(NSView *)subview {
    (void)splitView;
    if (subview == self.terminal) {
        return NO;
    }
    return YES;
}

- (void)viewDidAppear {
    [super viewDidAppear];
    if (!getenv("RILL_TEST_HIDE_CHROME")) {
        return;
    }
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!self->_navHidden) {
            [self toggleNav];
        }
    });
}

- (BOOL)sidebarsHidden {
    return _navHidden;
}

- (void)toggleNav {
    [self togglePaneNav:YES];
}

- (void)toggleInspector {
    [self togglePaneNav:NO];
}

- (void)togglePaneNav:(BOOL)nav {
    NSSplitView *split = (NSSplitView *)self.view;
    if (![split isKindOfClass:[NSSplitView class]] || split.subviews.count < 3) {
        return;
    }
    if (self.hideDetaches && self.terminal) {
        [self.terminal.window close];
        return;
    }
    if (nav) {
        _navHidden = !_navHidden;
    } else {
        _inspectorHidden = !_inspectorHidden;
    }
    [self applySplitPositions];
    if (self.terminal) {
        [self.view.window makeFirstResponder:self.terminal];
        [self.terminal returnToLiveViewport];
        [self.terminal writeTestHeartbeat];
    }
}

- (void)finishPane:(RillChromePane *)pane width:(CGFloat)width {
    pane.wantsLayer = YES;
    pane.layer.backgroundColor = RillRgbaColor(self.paneRgba).CGColor;
    pane.topInset = self.topInset;
    pane.freezeFrames = self.freezeY;
    if (self.freezeY) {
        pane.heading.frame = NSMakeRect(14, 680 - 36, width - 28, 16);
        pane.heading.autoresizingMask = NSViewMinYMargin | NSViewWidthSizable;
        CGFloat y = 680 - 68;
        for (NSView *row in pane.rows) {
            row.frame = NSMakeRect(8, y, width - 16, 28);
            row.autoresizingMask = NSViewMinYMargin | NSViewWidthSizable;
            y -= 32;
        }
    } else {
        [pane setNeedsLayout:YES];
        [pane layoutSubtreeIfNeeded];
    }
}

- (RillChromePane *)navPaneNamed:(NSString *)name {
    (void)name;
    RillChromePane *pane = [[RillChromePane alloc] initWithFrame:NSMakeRect(0, 0, kRillNavWidth, 680)];
    pane.rows = [NSMutableArray array];
    pane.topInset = self.topInset;
    pane.freezeFrames = self.freezeY;

    NSTextField *heading = [self sectionLabel:@"Workspaces"];
    heading.accessibilityIdentifier = @"chrome-left-heading";
    pane.heading = heading;
    [pane addSubview:heading];

    NSString *rowName = nil;
    NSString *ident = @"workspace-row-0";
    if (self.workspaceId != 0) {
        rowName = [NSString stringWithFormat:@"%llu", (unsigned long long)self.workspaceId];
        ident = [NSString stringWithFormat:@"workspace-id-%llu", (unsigned long long)self.workspaceId];
    } else {
        rowName = NSHomeDirectory().lastPathComponent ?: @"Rill";
    }
    NSView *row = [self iconRow:rowName symbol:@"folder" ident:ident];
    [pane.rows addObject:row];
    [pane addSubview:row];
    [self finishPane:pane width:kRillNavWidth];
    return pane;
}

- (RillChromePane *)inspectorPaneNamed:(NSString *)name {
    RillChromePane *pane = [[RillChromePane alloc] initWithFrame:NSMakeRect(0, 0, kRillInspectorWidth, 680)];
    pane.rows = [NSMutableArray array];
    pane.topInset = self.topInset;
    pane.freezeFrames = self.freezeY;

    NSTextField *heading = [self sectionLabel:name];
    heading.accessibilityIdentifier = @"host-indicator";
    pane.heading = heading;
    [pane addSubview:heading];

    NSView *changes = [self iconRow:@"Changes" symbol:@"plus.minus" ident:@"inspector-changes"];
    [pane.rows addObject:changes];
    [pane addSubview:changes];

    NSView *files = [self iconRow:@"Files" symbol:@"folder" ident:@"inspector-files"];
    [pane.rows addObject:files];
    [pane addSubview:files];

    NSTextField *agentsHead = [self sectionLabel:@"Agents"];
    agentsHead.accessibilityIdentifier = @"chrome-agents-heading";
    [pane.rows addObject:agentsHead];
    [pane addSubview:agentsHead];
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "fabricate_agent_row") == 0) {
        NSView *fake = [self iconRow:@"Review agent" symbol:@"cpu" ident:@"agent-row-fake"];
        [pane.rows addObject:fake];
        [pane addSubview:fake];
    }
    [self finishPane:pane width:kRillInspectorWidth];
    return pane;
}

- (NSTextField *)sectionLabel:(NSString *)text {
    NSTextField *field = [NSTextField labelWithString:text];
    CGFloat size = NSFont.systemFontSize;
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "tiny_chrome_font") == 0) {
        size = 9;
    }
    field.font = [NSFont systemFontOfSize:size weight:NSFontWeightSemibold];
    field.textColor = RillMutedBetween(self.paneRgba, self.fgRgba);
    field.drawsBackground = NO;
    field.selectable = NO;
    return field;
}

- (NSView *)iconRow:(NSString *)title symbol:(NSString *)symbol ident:(NSString *)ident {
    NSView *row = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 160, 28)];
    row.accessibilityIdentifier = ident;

    NSImageView *icon = [[NSImageView alloc] initWithFrame:NSMakeRect(6, 6, 16, 16)];
    if (@available(macOS 11.0, *)) {
        NSImage *image = [NSImage imageWithSystemSymbolName:symbol accessibilityDescription:title];
        image.size = NSMakeSize(13, 13);
        icon.image = image;
        icon.contentTintColor = RillMutedBetween(self.paneRgba, self.fgRgba);
    }
    icon.imageScaling = NSImageScaleProportionallyDown;
    [row addSubview:icon];

    NSTextField *label = [NSTextField labelWithString:title];
    label.font = [NSFont systemFontOfSize:NSFont.systemFontSize weight:NSFontWeightMedium];
    label.textColor = RillRgbaColor(self.fgRgba);
    label.drawsBackground = NO;
    label.selectable = NO;
    label.lineBreakMode = NSLineBreakByTruncatingTail;
    label.frame = NSMakeRect(28, 5, 120, 18);
    label.autoresizingMask = NSViewWidthSizable;
    [row addSubview:label];
    return row;
}

@end
