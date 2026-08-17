/* M2 chrome: nav | Chip 0 | inspector. AppKit only. The center subview is
 * TerminalView (Metal). Sidebars do not paint PTY bytes (ADR 0018). */

#import "ChromeHost.h"
#import "TerminalView.h"

static NSColor *RillChromeBackground(void) {
    return [NSColor colorWithCalibratedWhite:0.09 alpha:1.0];
}

static NSColor *RillChromeLabel(void) {
    return [NSColor colorWithCalibratedWhite:0.86 alpha:1.0];
}

static NSColor *RillChromeMuted(void) {
    return [NSColor colorWithCalibratedWhite:0.55 alpha:1.0];
}
static const CGFloat kRillNavWidth = 200.0;
static const CGFloat kRillInspectorWidth = 180.0;
static const CGFloat kRillNavMin = 160.0;
static const CGFloat kRillInspectorMin = 140.0;
static const CGFloat kRillCenterMin = 320.0;

@interface RillChromePane : NSView
@property (nonatomic, weak) TerminalView *terminal;
@end

@implementation RillChromePane
- (BOOL)acceptsFirstResponder {
    return NO;
}
- (void)mouseDown:(NSEvent *)event {
    (void)event;
    if (self.terminal) {
        [self.window makeFirstResponder:self.terminal];
    }
}
@end

@interface RillChromeController () <NSSplitViewDelegate>
@property (nonatomic, strong) TerminalView *terminal;
@property (nonatomic, assign) BOOL positioned;
@end

@implementation RillChromeController

- (instancetype)initWithTerminal:(TerminalView *)terminal {
    self = [super initWithNibName:nil bundle:nil];
    if (self) {
        _terminal = terminal;
    }
    return self;
}

- (void)loadView {
    NSSplitView *split = [[NSSplitView alloc] initWithFrame:NSMakeRect(0, 0, 1100, 680)];
    split.vertical = YES;
    split.dividerStyle = NSSplitViewDividerStyleThin;
    split.delegate = self;
    split.accessibilityIdentifier = @"chrome-split";

    NSString *homeName = NSHomeDirectory().lastPathComponent ?: @"Rill";
    RillChromePane *left = [self navPaneNamed:homeName];
    left.terminal = self.terminal;
    left.accessibilityIdentifier = @"chrome-left";

    self.terminal.accessibilityIdentifier = @"chrome-center";

    RillChromePane *right = [self inspectorPaneNamed:homeName];
    right.terminal = self.terminal;
    right.accessibilityIdentifier = @"chrome-right";

    [split addSubview:left];
    [split addSubview:self.terminal];
    [split addSubview:right];
    [split setHoldingPriority:NSLayoutPriorityDefaultHigh forSubviewAtIndex:0];
    [split setHoldingPriority:NSLayoutPriorityDefaultLow forSubviewAtIndex:1];
    [split setHoldingPriority:NSLayoutPriorityDefaultHigh forSubviewAtIndex:2];
    self.view = split;
}

- (void)viewDidLayout {
    [super viewDidLayout];
    if (self.positioned) {
        return;
    }
    NSSplitView *split = (NSSplitView *)self.view;
    CGFloat w = split.bounds.size.width;
    if (w < kRillNavMin + kRillCenterMin + kRillInspectorMin) {
        return;
    }
    [split setPosition:kRillNavWidth ofDividerAtIndex:0];
    [split setPosition:(w - kRillInspectorWidth) ofDividerAtIndex:1];
    self.positioned = YES;
}

- (CGFloat)splitView:(NSSplitView *)splitView
    constrainMinCoordinate:(CGFloat)proposed
              ofSubviewAt:(NSInteger)dividerIndex {
    (void)splitView;
    if (dividerIndex == 0) {
        return MAX(proposed, kRillNavMin);
    }
    return MAX(proposed, kRillNavMin + kRillCenterMin);
}

- (CGFloat)splitView:(NSSplitView *)splitView
    constrainMaxCoordinate:(CGFloat)proposed
              ofSubviewAt:(NSInteger)dividerIndex {
    CGFloat w = splitView.bounds.size.width;
    if (dividerIndex == 0) {
        return MIN(proposed, w - kRillInspectorMin - kRillCenterMin);
    }
    return MIN(proposed, w - kRillInspectorMin);
}

- (BOOL)splitView:(NSSplitView *)splitView canCollapseSubview:(NSView *)subview {
    (void)splitView;
    (void)subview;
    return NO;
}

- (RillChromePane *)navPaneNamed:(NSString *)name {
    RillChromePane *pane = [[RillChromePane alloc] initWithFrame:NSMakeRect(0, 0, kRillNavWidth, 680)];
    pane.wantsLayer = YES;
    pane.layer.backgroundColor = RillChromeBackground().CGColor;

    NSTextField *heading = [self sectionLabel:@"Workspaces"];
    heading.frame = NSMakeRect(14, 680 - 36, kRillNavWidth - 28, 16);
    heading.autoresizingMask = NSViewMinYMargin | NSViewWidthSizable;
    [pane addSubview:heading];

    NSView *row = [self iconRow:name symbol:@"folder" ident:@"workspace-row-0"];
    row.frame = NSMakeRect(8, 680 - 68, kRillNavWidth - 16, 28);
    row.autoresizingMask = NSViewMinYMargin | NSViewWidthSizable;
    [pane addSubview:row];
    return pane;
}

- (RillChromePane *)inspectorPaneNamed:(NSString *)name {
    RillChromePane *pane = [[RillChromePane alloc] initWithFrame:NSMakeRect(0, 0, kRillInspectorWidth, 680)];
    pane.wantsLayer = YES;
    pane.layer.backgroundColor = RillChromeBackground().CGColor;

    NSTextField *heading = [self sectionLabel:[NSString stringWithFormat:@"On %@", name]];
    heading.frame = NSMakeRect(14, 680 - 36, kRillInspectorWidth - 28, 16);
    heading.autoresizingMask = NSViewMinYMargin | NSViewWidthSizable;
    [pane addSubview:heading];

    NSView *changes = [self iconRow:@"Changes" symbol:@"plus.minus" ident:@"inspector-changes"];
    changes.frame = NSMakeRect(8, 680 - 68, kRillInspectorWidth - 16, 28);
    changes.autoresizingMask = NSViewMinYMargin | NSViewWidthSizable;
    [pane addSubview:changes];

    NSView *files = [self iconRow:@"Files" symbol:@"folder" ident:@"inspector-files"];
    files.frame = NSMakeRect(8, 680 - 100, kRillInspectorWidth - 16, 28);
    files.autoresizingMask = NSViewMinYMargin | NSViewWidthSizable;
    [pane addSubview:files];
    return pane;
}

- (NSTextField *)sectionLabel:(NSString *)text {
    NSTextField *field = [NSTextField labelWithString:text];
    field.font = [NSFont systemFontOfSize:11 weight:NSFontWeightSemibold];
    field.textColor = RillChromeMuted();
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
        icon.contentTintColor = RillChromeMuted();
    }
    icon.imageScaling = NSImageScaleProportionallyDown;
    [row addSubview:icon];

    NSTextField *label = [NSTextField labelWithString:title];
    label.font = [NSFont systemFontOfSize:13 weight:NSFontWeightMedium];
    label.textColor = RillChromeLabel();
    label.drawsBackground = NO;
    label.selectable = NO;
    label.lineBreakMode = NSLineBreakByTruncatingTail;
    label.frame = NSMakeRect(28, 5, 120, 18);
    label.autoresizingMask = NSViewWidthSizable;
    [row addSubview:label];
    return row;
}

@end
