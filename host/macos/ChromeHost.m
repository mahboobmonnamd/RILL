/* M2 chrome: nav | Chip 1 | inspector. AppKit only. The center subview is
 * TerminalView (Metal). Sidebars do not paint PTY bytes (ADR 0018). */

#import "ChromeHost.h"
#import "TerminalView.h"
#include "rill_ffi.h"
#import <QuartzCore/QuartzCore.h>
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

@protocol RillTabChrome <NSObject>
- (void)showTabAtIndex:(NSUInteger)idx;
- (void)closeTabButton:(id)sender;
- (void)newTabFromKernel;
@end

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
        NSTextField *label = nil;
        NSView *hint = nil;
        for (NSView *sub in row.subviews) {
            if ([sub.accessibilityIdentifier hasPrefix:@"chord-hint-"]) {
                hint = sub;
            } else if ([sub isKindOfClass:[NSTextField class]]) {
                label = (NSTextField *)sub;
            }
        }
        if (hint) {
            hint.frame = NSMakeRect(MAX(40.0, row.bounds.size.width - 48.0), 6, 44, 16);
        }
        if (label) {
            CGFloat right = hint ? NSMinX(hint.frame) - 8.0 : row.bounds.size.width - 8.0;
            label.frame = NSMakeRect(28, 5, MAX(16.0, right - 28.0), 18);
        }
        y += 32;
    }
}
@end

static NSTextField *RillChordHint(NSString *text, uint32_t fg) {
    NSTextField *h = [NSTextField labelWithString:text];
    h.font = [NSFont monospacedSystemFontOfSize:10 weight:NSFontWeightMedium];
    h.textColor = [RillRgbaColor(fg) colorWithAlphaComponent:0.72];
    h.alignment = NSTextAlignmentCenter;
    h.drawsBackground = YES;
    h.backgroundColor = [RillRgbaColor(fg) colorWithAlphaComponent:0.10];
    h.wantsLayer = YES;
    h.layer.cornerRadius = 3;
    h.selectable = NO;
    h.hidden = YES;
    return h;
}

@interface RillTabChip : NSView
@property (nonatomic, assign) NSUInteger index;
@property (nonatomic, assign) BOOL on;
@property (nonatomic, strong) NSTextField *titleField;
@property (nonatomic, strong) NSTextField *hint;
@property (nonatomic, strong) NSButton *closeButton;
@property (nonatomic, weak) id<RillTabChrome> target;
@end
@implementation RillTabChip
- (BOOL)isFlipped {
    return YES;
}
- (instancetype)initWithIndex:(NSUInteger)idx
                           on:(BOOL)on
                           fg:(uint32_t)fg {
    self = [super initWithFrame:NSMakeRect(0, 0, 160, 26)];
    if (self) {
        _index = idx;
        _on = on;
        self.wantsLayer = YES;
        self.layer.cornerRadius = 8;
        self.layer.borderWidth = on ? 1.0 : 0.0;
        self.layer.borderColor = [RillRgbaColor(fg) colorWithAlphaComponent:0.45].CGColor;
        self.layer.backgroundColor =
            on ? [RillRgbaColor(fg) colorWithAlphaComponent:0.28].CGColor
               : [RillRgbaColor(fg) colorWithAlphaComponent:0.05].CGColor;
        self.accessibilityIdentifier = [NSString stringWithFormat:@"kernel-tab-%lu", (unsigned long)idx];
        self.accessibilityRole = NSAccessibilityButtonRole;
        self.accessibilityLabel = @"Terminal";
        NSTextField *t = [[NSTextField alloc] initWithFrame:NSZeroRect];
        t.stringValue = @"Terminal";
        t.bezeled = NO;
        t.editable = NO;
        t.drawsBackground = NO;
        t.selectable = NO;
        t.font = [NSFont systemFontOfSize:NSFont.systemFontSize
                                   weight:on ? NSFontWeightMedium : NSFontWeightRegular];
        t.textColor = RillRgbaColor(fg);
        t.lineBreakMode = NSLineBreakByTruncatingTail;
        _titleField = t;
        [self addSubview:t];
        NSString *chord = idx < 8 ? [NSString stringWithFormat:@"⌘%lu", (unsigned long)(idx + 1)]
                                  : (idx == 8 ? @"⌘9" : @"");
        NSTextField *hint = RillChordHint(chord.length ? chord : @"", fg);
        hint.accessibilityIdentifier =
            [NSString stringWithFormat:@"chord-hint-tab-%lu", (unsigned long)idx];
        _hint = hint;
        [self addSubview:hint];
        NSImage *x = [NSImage imageWithSystemSymbolName:@"xmark"
                              accessibilityDescription:@"Close Tab"];
        NSButton *c = [NSButton buttonWithImage:x target:nil action:nil];
        c.bordered = NO;
        c.imagePosition = NSImageOnly;
        c.toolTip = @"Close Tab";
        c.accessibilityLabel = @"Close Tab";
        c.accessibilityIdentifier =
            [NSString stringWithFormat:@"rill-tab-close-%lu", (unsigned long)idx];
        _closeButton = c;
        [self addSubview:c];
    }
    return self;
}
- (void)layout {
    [super layout];
    CGFloat w = MAX(1.0, self.bounds.size.width);
    CGFloat h = MAX(1.0, self.bounds.size.height);
    CGFloat pad = 10.0;
    CGFloat closeW = self.closeButton.hidden ? 0.0 : 18.0;
    BOOL hintOn = !self.hint.hidden && self.hint.stringValue.length > 0;
    CGFloat hintSlot = 32.0;
    self.closeButton.frame = NSMakeRect(w - 8.0 - 18.0, MAX(0.0, (h - 18.0) / 2.0), 18.0, 18.0);
    self.hint.frame = NSMakeRect(pad, MAX(0.0, (h - 16.0) / 2.0), 28.0, 16.0);
    CGFloat titleX = hintOn ? pad + hintSlot : pad;
    CGFloat titleR = w - 8.0 - (closeW > 0.0 ? closeW + 4.0 : 0.0);
    self.titleField.frame =
        NSMakeRect(titleX, MAX(0.0, (h - 18.0) / 2.0), MAX(8.0, titleR - titleX), 18.0);
}
- (NSView *)hitTest:(NSPoint)point {
    NSView *hit = [super hitTest:point];
    if (hit == self.hint) {
        return self;
    }
    return hit;
}
- (void)mouseDown:(NSEvent *)event {
    (void)event;
    [self.target showTabAtIndex:self.index];
}
@end

@interface RillTabBar : NSView
@property (nonatomic, strong) NSMutableArray<RillTabChip *> *chips;
@property (nonatomic, strong) NSView *chipRow;
@property (nonatomic, strong) NSScrollView *chipScroll;
@property (nonatomic, strong) NSButton *plus;
@property (nonatomic, weak) id<RillTabChrome> chrome;
@property (nonatomic, assign) uint32_t fgRgba;
@property (nonatomic, assign) uint32_t paneRgba;
@property (nonatomic, assign) BOOL hintsOn;
@property (nonatomic, assign) NSUInteger selectedIndex;
@end
@implementation RillTabBar
- (BOOL)isFlipped {
    return YES;
}
- (instancetype)initWithFg:(uint32_t)fg pane:(uint32_t)pane chrome:(id)chrome {
    self = [super initWithFrame:NSMakeRect(0, 0, 700, 36)];
    if (self) {
        _fgRgba = fg;
        _paneRgba = pane;
        _chrome = chrome;
        _chips = [NSMutableArray array];
        self.wantsLayer = YES;
        self.layer.backgroundColor = RillRgbaColor(pane).CGColor;
        self.accessibilityIdentifier = @"chrome-tab-strip";
        NSView *row = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 400, 26)];
        row.wantsLayer = YES;
        _chipRow = row;
        NSScrollView *scroll = [[NSScrollView alloc] initWithFrame:NSMakeRect(0, 0, 400, 36)];
        scroll.drawsBackground = NO;
        scroll.hasHorizontalScroller = YES;
        scroll.hasVerticalScroller = NO;
        scroll.autohidesScrollers = YES;
        scroll.borderType = NSNoBorder;
        scroll.documentView = row;
        if (@available(macOS 11.0, *)) {
            scroll.automaticallyAdjustsContentInsets = NO;
            scroll.contentInsets = NSEdgeInsetsZero;
        }
        scroll.accessibilityIdentifier = @"chrome-tab-scroll";
        _chipScroll = scroll;
        [self addSubview:scroll];
        NSButton *plus = [NSButton buttonWithImage:[NSImage imageWithSystemSymbolName:@"plus"
                                                             accessibilityDescription:@"New Tab"]
                                            target:chrome
                                            action:@selector(newTabFromKernel)];
        plus.bordered = NO;
        plus.imagePosition = NSImageOnly;
        plus.toolTip = @"New Tab";
        plus.accessibilityLabel = @"New Tab";
        plus.accessibilityIdentifier = @"rill-tab-plus";
        _plus = plus;
        [self addSubview:plus];
        CALayer *edge = [CALayer layer];
        edge.backgroundColor = [RillRgbaColor(fg) colorWithAlphaComponent:0.14].CGColor;
        edge.name = @"tab-edge";
        [self.layer addSublayer:edge];
    }
    return self;
}
- (void)layout {
    [super layout];
    CGFloat w = self.bounds.size.width;
    CGFloat h = self.bounds.size.height;
    CGFloat pad = 10;
    CGFloat plusW = 28;
    CGFloat gap = 5;
    NSUInteger n = self.chips.count;
    const char *mut = getenv("RILL_MUTATE");
    BOOL noscroll = mut && strcmp(mut, "clip_tabs_no_scroll") == 0;
    BOOL plusFar = mut && strcmp(mut, "plus_at_window_trailing") == 0;
    CGFloat avail = MAX(80, w - pad * 2 - plusW - 6);
    const CGFloat kTabMin = 112;
    const CGFloat kTabMax = 200;
    CGFloat tw = kTabMax;
    if (n > 0) {
        CGFloat gaps = gap * (CGFloat)(n - 1);
        CGFloat fit = (avail - gaps) / (CGFloat)n;
        if (noscroll) {
            tw = MIN(kTabMax, MAX(72, fit));
        } else if (fit >= kTabMax) {
            tw = kTabMax;
        } else if (fit >= kTabMin) {
            tw = fit;
        } else {
            tw = kTabMin;
        }
    }
    CGFloat x = 0;
    for (RillTabChip *chip in self.chips) {
        chip.hint.hidden = !self.hintsOn || chip.hint.stringValue.length == 0;
        chip.frame = NSMakeRect(x, 4, tw, h - 10);
        [chip setNeedsLayout:YES];
        [chip layoutSubtreeIfNeeded];
        x += tw + gap;
    }
    CGFloat cluster = n == 0 ? 0 : x - gap;
    BOOL overflow = cluster > avail + 0.5;
    CGFloat scrollW = overflow ? avail : MAX(cluster, 1);
    self.chipRow.frame = NSMakeRect(0, 0, MAX(scrollW, cluster), h);
    self.chipScroll.frame = NSMakeRect(pad, 0, scrollW, h);
    self.chipScroll.hasHorizontalScroller = overflow;
    CGFloat plusX = pad + scrollW + 4;
    if (plusFar) {
        plusX = w - pad - plusW;
    }
    self.plus.frame = NSMakeRect(plusX, (h - 22) / 2, plusW, 22);
    for (CALayer *layer in self.layer.sublayers) {
        if ([layer.name isEqualToString:@"tab-edge"]) {
            layer.frame = NSMakeRect(0, h - 1, w, 1);
        }
    }
}
- (void)reloadCount:(NSUInteger)count selected:(NSUInteger)selected {
    for (RillTabChip *c in self.chips) {
        [c removeFromSuperview];
    }
    [self.chips removeAllObjects];
    self.selectedIndex = selected;
    for (NSUInteger i = 0; i < count; i++) {
        RillTabChip *chip = [[RillTabChip alloc] initWithIndex:i
                                                            on:i == selected
                                                            fg:self.fgRgba];
        chip.target = self.chrome;
        chip.closeButton.target = self.chrome;
        chip.closeButton.action = @selector(closeTabButton:);
        chip.closeButton.tag = (NSInteger)i;
        chip.hint.hidden = !self.hintsOn;
        const char *mut = getenv("RILL_MUTATE");
        if (mut && strcmp(mut, "skip_tab_close") == 0) {
            chip.closeButton.hidden = YES;
            chip.closeButton.accessibilityIdentifier = @"muted-close";
        }
        [self.chipRow addSubview:chip];
        [self.chips addObject:chip];
    }
    [self setNeedsLayout:YES];
    [self layoutSubtreeIfNeeded];
}

- (void)restyleSelected:(NSUInteger)selected {
    self.selectedIndex = selected;
    NSUInteger i = 0;
    for (RillTabChip *chip in self.chips) {
        BOOL on = i == selected;
        chip.on = on;
        chip.layer.borderWidth = on ? 1.0 : 0.0;
        chip.layer.borderColor = [RillRgbaColor(self.fgRgba) colorWithAlphaComponent:0.45].CGColor;
        chip.layer.backgroundColor =
            on ? [RillRgbaColor(self.fgRgba) colorWithAlphaComponent:0.28].CGColor
               : [RillRgbaColor(self.fgRgba) colorWithAlphaComponent:0.05].CGColor;
        chip.titleField.font = [NSFont systemFontOfSize:NSFont.systemFontSize
                                                 weight:on ? NSFontWeightMedium : NSFontWeightRegular];
        [chip setNeedsLayout:YES];
        i++;
    }
}
@end

@interface RillCenterHost : NSView
@property (nonatomic, strong) NSView *tabStrip;
@property (nonatomic, strong) NSView *terminalHost;
@end
@implementation RillCenterHost
- (BOOL)isFlipped {
    return YES;
}
- (void)layout {
    [super layout];
    CGFloat w = self.bounds.size.width;
    CGFloat h = self.bounds.size.height;
    CGFloat bar = 36.0;
    self.tabStrip.frame = NSMakeRect(0, 0, w, bar);
    self.terminalHost.frame = NSMakeRect(0, bar, w, MAX(8.0, h - bar));
    for (NSView *v in self.terminalHost.subviews) {
        v.frame = self.terminalHost.bounds;
    }
}
@end

@interface RillChromeController () <NSSplitViewDelegate, RillTabChrome>
@property (nonatomic, strong) TerminalView *terminal;
@property (nonatomic, strong) NSMutableArray<TerminalView *> *terminals;
@property (nonatomic, strong) RillCenterHost *centerHost;
@property (nonatomic, assign) NSUInteger selectedTab;
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
@property (nonatomic, strong) NSTextField *workspaceHint;
@property (nonatomic, strong) id flagsMonitor;
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
        _terminals = [NSMutableArray array];
        if (terminal) {
            [_terminals addObject:terminal];
        }
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

    RillCenterHost *center = [[RillCenterHost alloc] initWithFrame:NSMakeRect(0, 0, 700, 680)];
    RillTabBar *bar = [[RillTabBar alloc] initWithFg:self.fgRgba pane:self.paneRgba chrome:self];
    NSView *host = [[NSView alloc] initWithFrame:NSMakeRect(0, 36, 700, 644)];
    host.autoresizesSubviews = YES;
    [host addSubview:self.terminal];
    self.terminal.frame = host.bounds;
    self.terminal.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    center.tabStrip = bar;
    center.terminalHost = host;
    [center addSubview:bar];
    [center addSubview:host];
    self.centerHost = center;
    [bar reloadCount:self.terminals.count selected:0];

    RillChromePane *right = [self inspectorPaneNamed:self.hostIdentity];
    right.terminal = self.terminal;
    right.accessibilityIdentifier = @"chrome-right";

    [split addSubview:left];
    [split addSubview:center];
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
                                             selector:@selector(newTabFromKernel)
                                                 name:@"RillNewTab"
                                               object:nil];
    [[NSNotificationCenter defaultCenter] addObserver:self
                                             selector:@selector(closeInnermostPresentation)
                                                 name:@"RillCloseInnermost"
                                               object:nil];
    [[NSNotificationCenter defaultCenter] addObserver:self
                                             selector:@selector(selectTabFromNote:)
                                                 name:@"RillSelectTab"
                                               object:nil];
    [[NSNotificationCenter defaultCenter] addObserver:self
                                             selector:@selector(chordHintsFromNote:)
                                                 name:@"RillChordHints"
                                               object:nil];
    __weak RillChromeController *weakSelf = self;
    self.flagsMonitor =
        [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskFlagsChanged
                                              handler:^NSEvent *(NSEvent *event) {
                                                RillChromeController *strong = weakSelf;
                                                if (strong) {
                                                    [strong applyChordHintsFromFlags:event.modifierFlags];
                                                }
                                                return event;
                                              }];
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
    if (subview == self.centerHost) {
        return NO;
    }
    return YES;
}

- (void)viewDidAppear {
    [super viewDidAppear];
    if (getenv("RILL_TEST_NEW_TAB")) {
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.45 * NSEC_PER_SEC)),
                       dispatch_get_main_queue(), ^{
                         [self newTabFromKernel];
                         if (getenv("RILL_TEST_SELECT_TAB")) {
                             dispatch_after(dispatch_time(DISPATCH_TIME_NOW,
                                                          (int64_t)(0.55 * NSEC_PER_SEC)),
                                            dispatch_get_main_queue(), ^{
                                              [[NSNotificationCenter defaultCenter]
                                                  postNotificationName:@"RillSelectTab"
                                                                object:nil
                                                              userInfo:@{@"i" : @0}];
                                            });
                         }
                         if (getenv("RILL_TEST_CLOSE_TAB")) {
                             dispatch_after(dispatch_time(DISPATCH_TIME_NOW,
                                                          (int64_t)(0.45 * NSEC_PER_SEC)),
                                            dispatch_get_main_queue(), ^{
                                              [self closeInnermostPresentation];
                                            });
                         }
                       });
    }
    const char *many = getenv("RILL_TEST_MANY_TABS");
    if (many && many[0]) {
        int n = atoi(many);
        if (n > 1) {
            [self spawnExtraTabs:(n - 1)];
        }
    }
    if (getenv("RILL_TEST_CMD_HINT")) {
        dispatch_async(dispatch_get_main_queue(), ^{
            [self applyChordHintsFromFlags:NSEventModifierFlagCommand];
        });
    }
    if (!getenv("RILL_TEST_HIDE_CHROME")) {
        return;
    }
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!self->_navHidden) {
            [self toggleNav];
        }
    });
}

- (void)spawnExtraTabs:(int)left {
    if (left <= 0) {
        return;
    }
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.18 * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), ^{
                     [self newTabFromKernel];
                     [self spawnExtraTabs:(left - 1)];
                   });
}

- (void)dealloc {
    if (self.flagsMonitor) {
        [NSEvent removeMonitor:self.flagsMonitor];
    }
}

- (void)chordHintsFromNote:(NSNotification *)note {
    NSNumber *on = note.userInfo[@"on"];
    [self applyChordHintsFromFlags:on.boolValue ? NSEventModifierFlagCommand : 0];
}

- (void)applyChordHintsFromFlags:(NSEventModifierFlags)flags {
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "skip_cmd_hints") == 0) {
        return;
    }
    BOOL cmd = (flags & NSEventModifierFlagCommand) != 0;
    RillTabBar *bar = (RillTabBar *)self.centerHost.tabStrip;
    bar.hintsOn = cmd;
    [bar setNeedsLayout:YES];
    self.workspaceHint.hidden = !cmd;
    if (self.terminal) {
        [self.terminal writeTestHeartbeat];
    }
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

- (void)reloadTabBar {
    RillTabBar *bar = (RillTabBar *)self.centerHost.tabStrip;
    [bar reloadCount:self.terminals.count selected:self.selectedTab];
}

- (void)showTabAtIndex:(NSUInteger)idx {
    if (idx >= self.terminals.count) {
        return;
    }
    self.selectedTab = idx;
    for (NSUInteger i = 0; i < self.terminals.count; i++) {
        TerminalView *tv = self.terminals[i];
        tv.hidden = i != idx;
        tv.accessibilityIdentifier = i == idx ? @"chrome-center" : @"pane-surface";
    }
    self.terminal = self.terminals[idx];
    RillTabBar *bar = (RillTabBar *)self.centerHost.tabStrip;
    if (bar.chips.count == self.terminals.count) {
        [bar restyleSelected:idx];
    } else {
        [self reloadTabBar];
    }
    [self.view.window makeFirstResponder:self.terminal];
    [self.terminal writeTestHeartbeat];
}

- (void)selectTabFromNote:(NSNotification *)note {
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "skip_tab_index_keys") == 0) {
        return;
    }
    NSNumber *i = note.userInfo[@"i"];
    if (i) {
        [self showTabAtIndex:i.unsignedIntegerValue];
    }
}

- (void)closeTabButton:(id)sender {
    [self closeTabAtIndex:(NSUInteger)[sender tag]];
}

- (void)closeTabAtIndex:(NSUInteger)idx {
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "always_close_window") == 0) {
        [self.view.window orderOut:nil];
        [self.terminal writeTestHeartbeat];
        return;
    }
    if (self.terminals.count > 1) {
        if (idx >= self.terminals.count) {
            idx = self.terminals.count - 1;
        }
        TerminalView *tv = self.terminals[idx];
        [tv removeFromSuperview];
        [self.terminals removeObjectAtIndex:idx];
        NSUInteger next = idx == 0 ? 0 : idx - 1;
        if (next >= self.terminals.count) {
            next = self.terminals.count - 1;
        }
        self.selectedTab = next;
        [self reloadTabBar];
        [self showTabAtIndex:next];
        return;
    }
    [self.view.window performClose:nil];
}

- (void)closeInnermostPresentation {
    [self closeTabAtIndex:self.selectedTab];
}

- (void)newTabFromKernel {
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "chrome_invents_tab") == 0) {
        RillTabBar *bar = (RillTabBar *)self.centerHost.tabStrip;
        [bar reloadCount:self.terminals.count + 1 selected:self.selectedTab];
        if (self.terminal) {
            [self.terminal writeTestHeartbeat];
        }
        return;
    }
    uint64_t leaf = 0;
    int n = rill_nav_new_tab(NULL, &leaf);
    if (n < 2 || leaf == 0) {
        return;
    }
    RillClient *client = rill_client_connect_leaf(NULL, leaf);
    if (!client) {
        return;
    }
    TerminalView *tv = [[TerminalView alloc] initWithClient:client];
    if (!tv) {
        rill_client_free(client);
        return;
    }
    tv.frame = self.centerHost.terminalHost.bounds;
    tv.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    [self.centerHost.terminalHost addSubview:tv];
    [self.terminals addObject:tv];
    [self showTabAtIndex:self.terminals.count - 1];
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
    NSTextField *wsHint = RillChordHint(@"⌥⌘1", self.fgRgba);
    wsHint.accessibilityIdentifier = @"chord-hint-ws";
    wsHint.autoresizingMask = NSViewMinXMargin;
    wsHint.frame = NSMakeRect(kRillNavWidth - 58, 5, 44, 16);
    [row addSubview:wsHint];
    self.workspaceHint = wsHint;
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
    label.frame = NSMakeRect(28, 5, 80, 18);
    label.autoresizingMask = NSViewWidthSizable;
    [row addSubview:label];
    return row;
}

@end
