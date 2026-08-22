#import "FlowHost.h"

static NSColor *RillFlowRgba(uint32_t rgba) {
    return [NSColor colorWithSRGBRed:((rgba >> 24) & 0xff) / 255.0
                               green:((rgba >> 16) & 0xff) / 255.0
                                blue:((rgba >> 8) & 0xff) / 255.0
                               alpha:1.0];
}

static NSString *RillFlowOneLine(NSString *text) {
    if (!text || text.length == 0) {
        return @"";
    }
    NSString *line = [[text componentsSeparatedByCharactersInSet:[NSCharacterSet newlineCharacterSet]]
        componentsJoinedByString:@" "];
    return [line stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]];
}

@interface RillFlowBlockCard : NSView
@property (nonatomic, strong) NSTextField *badge;
@property (nonatomic, strong) NSTextField *title;
@property (nonatomic, strong) NSTextField *meta;
@property (nonatomic, strong) NSScrollView *bodyScroll;
@property (nonatomic, strong) NSTextView *bodyText;
@property (nonatomic, assign) CGFloat bodyVisibleHeight;
@end
@implementation RillFlowBlockCard
- (BOOL)isFlipped {
    return YES;
}
- (void)layout {
    [super layout];
    CGFloat w = self.bounds.size.width;
    self.badge.frame = NSMakeRect(14, 12, 22, 18);
    self.meta.frame = NSMakeRect(MAX(80, w - 144), 12, 130, 18);
    self.title.frame = NSMakeRect(44, 10, MAX(12, w - 194), 20);
    CGFloat bodyH = MAX(0, self.bodyVisibleHeight);
    self.bodyScroll.frame = NSMakeRect(14, 34, MAX(8, w - 28), bodyH);
}
@end

@interface RillFlowDoc : NSView
@end
@implementation RillFlowDoc
- (BOOL)isFlipped {
    return YES;
}
@end

@interface RillFlowHost () <NSTextFieldDelegate>
@property (nonatomic, strong) NSScrollView *scroll;
@property (nonatomic, strong) NSView *document;
@property (nonatomic, strong) NSView *composer;
@property (nonatomic, strong) NSTextField *field;
@property (nonatomic, strong) NSButton *run;
@property (nonatomic, strong) NSButton *flowMode;
@property (nonatomic, strong) NSButton *flowBolt;
@property (nonatomic, strong) NSButton *flowScope;
@property (nonatomic, strong) NSButton *scope;
@property (nonatomic, strong) NSButton *shellChip;
@property (nonatomic, strong) NSTextField *empty;
@property (nonatomic, copy) NSString *fontFamily;
@property (nonatomic, assign) CGFloat fontSize;
@property (nonatomic, assign) uint32_t bgRgba;
@property (nonatomic, assign) uint32_t fgRgba;
@property (nonatomic, strong) NSMutableArray<NSView *> *cards;
@property (nonatomic, assign) NSUInteger maxVisibleCards;
@end

@implementation RillFlowHost

- (void)styleChipButton:(NSButton *)button emphasized:(BOOL)emphasized {
    button.wantsLayer = YES;
    button.layer.cornerRadius = 7;
    button.layer.borderWidth = 1;
    button.layer.borderColor = [[RillFlowRgba(self.fgRgba) colorWithAlphaComponent:0.08] CGColor];
    button.layer.backgroundColor =
        [[RillFlowRgba(self.fgRgba) colorWithAlphaComponent:(emphasized ? 0.10 : 0.045)] CGColor];
    button.bordered = NO;
    button.font = [self uiFontOfSize:11 bold:emphasized];
}

- (NSScrollView *)makeBodyScrollWithText:(NSString *)text alpha:(CGFloat)alpha {
    NSScrollView *scroll = [[NSScrollView alloc] initWithFrame:NSZeroRect];
    scroll.drawsBackground = NO;
    scroll.hasVerticalScroller = NO;
    scroll.hasHorizontalScroller = NO;
    scroll.autohidesScrollers = YES;
    scroll.borderType = NSNoBorder;
    NSTextView *tv = [[NSTextView alloc] initWithFrame:NSZeroRect];
    tv.editable = NO;
    tv.selectable = YES;
    tv.drawsBackground = NO;
    tv.usesFontPanel = NO;
    tv.usesFindBar = YES;
    tv.textContainerInset = NSMakeSize(0, 2);
    tv.textContainer.widthTracksTextView = YES;
    tv.verticallyResizable = YES;
    tv.horizontallyResizable = NO;
    tv.textContainer.containerSize = NSMakeSize(CGFLOAT_MAX, CGFLOAT_MAX);
    tv.font = [self monoFontOfSize:MAX(11, self.fontSize - 1)];
    tv.textColor = [RillFlowRgba(self.fgRgba) colorWithAlphaComponent:alpha];
    tv.string = text.length ? text : @"";
    scroll.documentView = tv;
    return scroll;
}

- (instancetype)initWithFrame:(NSRect)frame
                       client:(RillClient *)client
                   fontFamily:(NSString *)family
                     fontSize:(CGFloat)size
                           bg:(uint32_t)bg
                           fg:(uint32_t)fg {
    self = [super initWithFrame:frame];
    if (!self) {
        return nil;
    }
    _client = client;
    _fontFamily = [family copy];
    _fontSize = size > 1 ? size : 13;
    _bgRgba = bg;
    _fgRgba = fg;
    _cards = [NSMutableArray array];
    _maxVisibleCards = 4;
    self.wantsLayer = YES;
    self.layer.backgroundColor = RillFlowRgba(bg).CGColor;
    self.accessibilityIdentifier = @"flow-host";

    _scroll = [[NSScrollView alloc] initWithFrame:NSZeroRect];
    _scroll.hasVerticalScroller = YES;
    _scroll.drawsBackground = NO;
    _scroll.borderType = NSNoBorder;
    _document = [[RillFlowDoc alloc] initWithFrame:NSZeroRect];
    _document.wantsLayer = YES;
    _scroll.documentView = _document;
    [self addSubview:_scroll];

    _empty = [NSTextField labelWithString:@"Run a command to create a block. Older output is compacted automatically."];
    _empty.textColor = [RillFlowRgba(fg) colorWithAlphaComponent:0.45];
    _empty.font = [self uiFontOfSize:12 bold:NO];
    _empty.accessibilityIdentifier = @"flow-empty";
    [_document addSubview:_empty];

    _composer = [[NSView alloc] initWithFrame:NSZeroRect];
    _composer.wantsLayer = YES;
    _composer.layer.backgroundColor = [[RillFlowRgba(fg) colorWithAlphaComponent:0.03] CGColor];
    _composer.layer.cornerRadius = 8;
    _composer.layer.borderWidth = 1;
    _composer.layer.borderColor = [[RillFlowRgba(fg) colorWithAlphaComponent:0.09] CGColor];
    _composer.accessibilityIdentifier = @"flow-composer";

    NSTextField *prompt = [NSTextField labelWithString:@"$"];
    prompt.font = [self monoFontOfSize:_fontSize];
    prompt.textColor = [RillFlowRgba(fg) colorWithAlphaComponent:0.95];
    prompt.tag = 91;
    [_composer addSubview:prompt];

    _field = [[NSTextField alloc] initWithFrame:NSZeroRect];
    _field.placeholderString = @"Type a command";
    _field.font = [self monoFontOfSize:_fontSize];
    _field.textColor = RillFlowRgba(fg);
    _field.backgroundColor = [NSColor clearColor];
    _field.bezeled = NO;
    _field.bordered = NO;
    _field.focusRingType = NSFocusRingTypeNone;
    _field.delegate = self;
    _field.accessibilityIdentifier = @"flow-composer-field";
    [_composer addSubview:_field];

    _run = [NSButton buttonWithTitle:@"⌘⏎ Run" target:self action:@selector(run:)];
    _run.bezelStyle = NSBezelStyleRounded;
    _run.keyEquivalent = @"\r";
    _run.accessibilityIdentifier = @"flow-composer-run";
    _run.wantsLayer = YES;
    _run.layer.cornerRadius = 7;
    _run.layer.borderWidth = 1;
    _run.layer.borderColor = [[RillFlowRgba(fg) colorWithAlphaComponent:0.14] CGColor];
    _run.layer.backgroundColor = [[RillFlowRgba(fg) colorWithAlphaComponent:0.12] CGColor];
    _run.font = [self uiFontOfSize:11 bold:YES];
    [_composer addSubview:_run];

    _flowMode = [NSButton buttonWithTitle:@"Flow" target:self action:@selector(selectFlow:)];
    _flowMode.bezelStyle = NSBezelStyleRounded;
    _flowMode.accessibilityIdentifier = @"flow-mode-flow";
    [self styleChipButton:_flowMode emphasized:YES];
    [_composer addSubview:_flowMode];

    _flowBolt = [NSButton buttonWithTitle:@"⚡" target:self action:@selector(selectFlow:)];
    _flowBolt.bezelStyle = NSBezelStyleRounded;
    _flowBolt.accessibilityIdentifier = @"flow-mode-bolt";
    [self styleChipButton:_flowBolt emphasized:YES];
    [_composer addSubview:_flowBolt];

    _flowScope = [NSButton buttonWithTitle:@"⌂" target:nil action:nil];
    _flowScope.bezelStyle = NSBezelStyleRounded;
    _flowScope.enabled = NO;
    _flowScope.accessibilityIdentifier = @"flow-scope-home";
    [self styleChipButton:_flowScope emphasized:NO];
    [_composer addSubview:_flowScope];

    _scope = [NSButton buttonWithTitle:@"~" target:nil action:nil];
    _scope.bezelStyle = NSBezelStyleRounded;
    _scope.enabled = NO;
    _scope.accessibilityIdentifier = @"flow-scope";
    [self styleChipButton:_scope emphasized:NO];
    [_composer addSubview:_scope];

    NSString *shellTitle = [NSString stringWithFormat:@"%@ ▾", self.shellLabel];
    _shellChip = [NSButton buttonWithTitle:shellTitle target:nil action:nil];
    _shellChip.bezelStyle = NSBezelStyleRounded;
    _shellChip.enabled = NO;
    _shellChip.font = [self monoFontOfSize:11];
    _shellChip.accessibilityIdentifier = @"flow-shell-chip";
    [self styleChipButton:_shellChip emphasized:NO];
    [_composer addSubview:_shellChip];

    [self addSubview:_composer];
    [self syncModeButtons];
    [self reloadFromClient];
    return self;
}

- (NSString *)shellLabel {
    const char *sh = getenv("SHELL");
    if (!sh || !sh[0]) {
        return @"shell";
    }
    NSString *p = [NSString stringWithUTF8String:sh];
    return p.lastPathComponent ?: @"shell";
}

- (NSFont *)uiFontOfSize:(CGFloat)size bold:(BOOL)bold {
    NSFont *f = [NSFont systemFontOfSize:size weight:bold ? NSFontWeightSemibold : NSFontWeightRegular];
    return f;
}

- (NSFont *)monoFontOfSize:(CGFloat)size {
    if (self.fontFamily.length) {
        NSFont *named = [NSFont fontWithName:self.fontFamily size:size];
        if (named) {
            return named;
        }
    }
    return [NSFont systemFontOfSize:size];
}

- (BOOL)isFlipped {
    return YES;
}

- (BOOL)composerVisible {
    return !self.hidden;
}

- (void)layout {
    [super layout];
    CGFloat w = self.bounds.size.width;
    CGFloat h = self.bounds.size.height;
    CGFloat composerH = 52;
    self.composer.frame = NSMakeRect(16, h - composerH - 12, MAX(80, w - 32), composerH);
    NSView *prompt = [self.composer viewWithTag:91];
    self.flowBolt.frame = NSMakeRect(10, 14, 26, 24);
    self.flowMode.frame = NSMakeRect(40, 14, 46, 24);
    self.flowScope.frame = NSMakeRect(90, 14, 26, 24);
    self.scope.frame = NSMakeRect(120, 14, 26, 24);
    self.shellChip.frame = NSMakeRect(self.composer.bounds.size.width - 176, 14, 84, 24);
    prompt.frame = NSMakeRect(154, 16, 16, 20);
    self.field.frame = NSMakeRect(172, 14, MAX(40, self.composer.bounds.size.width - 358), 24);
    self.run.frame = NSMakeRect(self.composer.bounds.size.width - 96, 13, 82, 26);
    self.scroll.frame = NSMakeRect(0, 8, w, MAX(40, h - composerH - 26));
    [self layoutCards];
}

- (void)layoutCards {
    CGFloat w = MAX(120, self.scroll.bounds.size.width - 8);
    CGFloat y = 12;
    CGFloat inner = w - 44;
    if (self.cards.count == 0) {
        self.empty.hidden = NO;
        self.empty.frame = NSMakeRect(24, 24, inner, 40);
        y = 80;
    } else {
        self.empty.hidden = YES;
    }
    for (NSView *view in self.cards) {
        RillFlowBlockCard *card = (RillFlowBlockCard *)view;
        CGFloat bodyMax = 168;
        CGFloat bodyMin = 48;
        NSString *body = card.bodyText.string ?: @"";
        NSDictionary *attrs = @{NSFontAttributeName : card.bodyText.font ?: [self monoFontOfSize:11]};
        CGRect r = [body boundingRectWithSize:NSMakeSize(inner - 42, CGFLOAT_MAX)
                                      options:NSStringDrawingUsesLineFragmentOrigin |
                                              NSStringDrawingUsesFontLeading
                                   attributes:attrs];
        CGFloat textH = ceil(r.size.height) + 8;
        CGFloat bodyVisible = MIN(bodyMax, MAX(bodyMin, textH));
        card.bodyVisibleHeight = bodyVisible;
        card.bodyScroll.hasVerticalScroller = textH > bodyVisible + 1;
        CGFloat ch = 34 + bodyVisible + 12;
        card.frame = NSMakeRect(22, y, inner, ch);
        y += ch + 10;
    }
    self.document.frame = NSMakeRect(0, 0, w, MAX(y + 16, self.scroll.bounds.size.height));
}

- (void)syncModeButtons {
    self.flowMode.enabled = NO;
    self.flowMode.alphaValue = 1.0;
    self.flowBolt.enabled = NO;
    self.flowBolt.alphaValue = 1.0;
}

- (void)selectFlow:(id)sender {
    (void)sender;
    if (self.rawFallback) {
        self.rawFallback = NO;
        [[NSNotificationCenter defaultCenter] postNotificationName:@"RillFlowRawChanged" object:self];
    }
}

- (void)run:(id)sender {
    (void)sender;
    if (!self.client) {
        return;
    }
    NSString *text = self.field.stringValue ?: @"";
    if (text.length == 0) {
        return;
    }
    NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
    if (rill_client_composer_submit(self.client, data.bytes, data.length) != 0) {
        NSLog(@"Rill: composer submit: %s", rill_client_last_error() ?: "error");
        return;
    }
    self.field.stringValue = @"";
    [self reloadFromClient];
}

- (void)controlTextDidEndEditing:(NSNotification *)obj {
    (void)obj;
}

- (BOOL)control:(NSControl *)control textView:(NSTextView *)textView doCommandBySelector:(SEL)commandSelector {
    (void)control;
    (void)textView;
    if (commandSelector == @selector(insertNewline:)) {
        [self run:nil];
        return YES;
    }
    return NO;
}

- (void)reloadFromClient {
    for (NSView *card in self.cards) {
        [card removeFromSuperview];
    }
    [self.cards removeAllObjects];
    if (!self.client) {
        [self layoutCards];
        return;
    }
    uint32_t n = rill_client_flow_count(self.client);
    NSMutableArray<NSDictionary *> *items = [NSMutableArray array];
    NSMutableString *ambient = [NSMutableString string];
    NSMutableString *pendingTitle = nil;
    NSMutableString *pendingBody = nil;
    BOOL have = NO;
    for (uint32_t i = 0; i < n; i++) {
        uint32_t kind = 0;
        const uint8_t *ptr = NULL;
        size_t len = 0;
        if (rill_client_flow_item(self.client, i, &kind, &ptr, &len) != 0) {
            continue;
        }
        NSString *text = [[NSString alloc] initWithBytes:ptr length:len encoding:NSUTF8StringEncoding]
                             ?: @"";
        if (kind == 1) {
            if (have) {
                [items addObject:@{
                    @"title" : pendingTitle ?: @"",
                    @"body" : pendingBody ?: @"",
                    @"input" : @YES,
                    @"meta" : @"Running"
                }];
            }
            NSString *oneline = RillFlowOneLine(text);
            if (oneline.length) {
                pendingTitle = [oneline mutableCopy];
            } else {
                pendingTitle = [@"" mutableCopy];
            }
            pendingBody = [NSMutableString string];
            have = YES;
        } else {
            if (!have) {
                if (ambient.length > 0) {
                    [ambient appendString:@"\n"];
                }
                [ambient appendString:text];
            } else {
                if (pendingBody.length) {
                    [pendingBody appendString:@"\n"];
                }
                [pendingBody appendString:text];
            }
        }
    }
    if (ambient.length > 0) {
        [items addObject:@{
            @"title" : @"terminal region",
            @"body" : ambient,
            @"input" : @NO,
            @"meta" : @"Output"
        }];
    }
    if (have) {
        [items addObject:@{
            @"title" : pendingTitle ?: @"",
            @"body" : pendingBody ?: @"",
            @"input" : @YES,
            @"meta" : @"Running"
        }];
    }

    NSUInteger total = items.count;
    NSUInteger keep = self.maxVisibleCards;
    NSUInteger start = total > keep ? total - keep : 0;
    if (start > 0) {
        NSString *summary = [NSString stringWithFormat:@"%lu older blocks hidden", (unsigned long)start];
        [self addSummaryCard:summary];
    }
    for (NSUInteger i = start; i < total; i++) {
        NSDictionary *it = items[i];
        [self addCardTitle:it[@"title"]
                      body:it[@"body"]
                     input:[it[@"input"] boolValue]
                      meta:it[@"meta"]];
    }
    [self setNeedsLayout:YES];
    [self layoutCards];
}

- (void)addSummaryCard:(NSString *)text {
    RillFlowBlockCard *card = [[RillFlowBlockCard alloc] initWithFrame:NSZeroRect];
    card.wantsLayer = YES;
    card.layer.cornerRadius = 8;
    card.layer.backgroundColor = [[RillFlowRgba(self.fgRgba) colorWithAlphaComponent:0.02] CGColor];
    card.layer.borderWidth = 1;
    card.layer.borderColor = [[RillFlowRgba(self.fgRgba) colorWithAlphaComponent:0.08] CGColor];

    NSTextField *badge = [NSTextField labelWithString:@"…"];
    badge.alignment = NSTextAlignmentCenter;
    badge.font = [self uiFontOfSize:11 bold:YES];
    badge.textColor = [RillFlowRgba(self.fgRgba) colorWithAlphaComponent:0.82];
    badge.wantsLayer = YES;
    badge.layer.backgroundColor = [[RillFlowRgba(self.fgRgba) colorWithAlphaComponent:0.06] CGColor];
    badge.layer.cornerRadius = 5;
    card.badge = badge;
    [card addSubview:badge];

    NSTextField *t = [NSTextField labelWithString:text ?: @"Older blocks hidden"];
    t.font = [self uiFontOfSize:12 bold:YES];
    t.textColor = [RillFlowRgba(self.fgRgba) colorWithAlphaComponent:0.86];
    t.lineBreakMode = NSLineBreakByTruncatingTail;
    card.title = t;
    [card addSubview:t];

    NSTextField *m = [NSTextField labelWithString:@"Compact"];
    m.font = [self uiFontOfSize:11 bold:NO];
    m.alignment = NSTextAlignmentRight;
    m.textColor = [RillFlowRgba(self.fgRgba) colorWithAlphaComponent:0.55];
    card.meta = m;
    [card addSubview:m];

    NSScrollView *scroll = [self makeBodyScrollWithText:@"" alpha:0.72];
    card.bodyScroll = scroll;
    card.bodyText = (NSTextView *)scroll.documentView;
    [card addSubview:scroll];
    card.accessibilityIdentifier = @"flow-summary-block";
    [self.document addSubview:card];
    [self.cards addObject:card];
}

- (void)addCardTitle:(NSString *)title body:(NSString *)body input:(BOOL)input meta:(NSString *)meta {
    RillFlowBlockCard *card = [[RillFlowBlockCard alloc] initWithFrame:NSZeroRect];
    card.wantsLayer = YES;
    card.layer.cornerRadius = 10;
    card.layer.backgroundColor = [[RillFlowRgba(self.fgRgba) colorWithAlphaComponent:0.03] CGColor];
    card.layer.borderWidth = 1;
    card.layer.borderColor = [[RillFlowRgba(self.fgRgba) colorWithAlphaComponent:0.09] CGColor];

    uint32_t fill = input ? 0x34d399ff : 0x93a4bbff;
    if (body.length && [body.lowercaseString containsString:@"error"]) {
        fill = 0xf87171ff;
    }
    NSTextField *badge = [NSTextField labelWithString:input ? @"$" : @">"];
    badge.alignment = NSTextAlignmentCenter;
    badge.font = [self uiFontOfSize:11 bold:YES];
    badge.textColor = RillFlowRgba(fill);
    badge.wantsLayer = YES;
    badge.layer.backgroundColor = [[RillFlowRgba(fill) colorWithAlphaComponent:0.13] CGColor];
    badge.layer.cornerRadius = 5;
    card.badge = badge;
    [card addSubview:badge];

    NSString *resolvedTitle = title.length ? title : @"(empty)";
    NSTextField *t = [NSTextField labelWithString:resolvedTitle];
    t.font = [self monoFontOfSize:MAX(11, self.fontSize - 1)];
    t.textColor = RillFlowRgba(self.fgRgba);
    t.lineBreakMode = NSLineBreakByTruncatingTail;
    card.title = t;
    [card addSubview:t];

    NSTextField *m = [NSTextField labelWithString:meta.length ? meta : (input ? @"Running" : @"Output")];
    m.font = [self uiFontOfSize:11 bold:NO];
    m.alignment = NSTextAlignmentRight;
    m.textColor = [RillFlowRgba(self.fgRgba) colorWithAlphaComponent:0.58];
    card.meta = m;
    [card addSubview:m];

    NSScrollView *scroll = [self makeBodyScrollWithText:(body.length ? body : @"") alpha:0.72];
    card.bodyScroll = scroll;
    card.bodyText = (NSTextView *)scroll.documentView;
    [card addSubview:scroll];
    card.accessibilityIdentifier = input ? @"flow-command-block" : @"flow-unstructured-block";
    [self.document addSubview:card];
    [self.cards addObject:card];
}

@end
