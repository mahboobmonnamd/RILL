/* Chip 0 presenter: glyph atlas + one instanced Metal draw. Socket readiness
 * feeds the VT (ADR 0003 D2). Surface is toggleFullScreen + opaque CAMetalLayer
 * (direct-to-display). Present is echo-only, one in flight. A CADisplayLink
 * supplies targetTimestamp so the echo can late-latch this vsync; it does not
 * take a drawable. Keystrokes pump+present on the same stack as keyDown so
 * the echo does not wait for a second runloop turn. No CAMetalDisplayLink.
 */

#import "TerminalView.h"
#import <CoreText/CoreText.h>
#import <Metal/Metal.h>
#import <MetalKit/MetalKit.h>
#import <QuartzCore/QuartzCore.h>
#import <ApplicationServices/ApplicationServices.h>
#include <simd/simd.h>
#include <math.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* US ANSI virtual keycodes for a–z. CGEventCreateKeyboardEvent(NULL, 0, …)
 * always posts 'a'; CGEventKeyboardSetUnicodeString did not populate
 * NSEvent.characters on PostToPid, so T-NFR hid discarded every sample
 * while --nfr-key=app (NSEvent sendEvent) painted 999/1000. */
static CGKeyCode rill_vk_ansi_letter(uint32_t cp) {
    static const CGKeyCode kVk[26] = {0,  11, 8,  2,  14, 3,  5,  4,  34, 38, 40,
                                      37, 46, 45, 31, 35, 12, 15, 1,  17, 32, 9,
                                      13, 7,  16, 6};
    if (cp < 'a' || cp > 'z') {
        return 0;
    }
    return kVk[cp - 'a'];
}

// ---------------------------------------------------------------- shaders
//
// One instance per cell. The fragment shader returns mix(bg, fg, atlasAlpha),
// so background and glyph come out of a single pass — instances tile the grid
// exactly, so nothing overlaps (ADR 0003 D1).
//
// Adjacent-literal concatenation: raw string literals are C++/ObjC++ only and
// this is a .m file.

static NSString *const kShaderSource =
    @"#include <metal_stdlib>\n"
    @"using namespace metal;\n"
    @"struct Uniforms { float2 viewport; float2 cellPx; uint cols; };\n"
    @"struct Instance {\n"
    @"  float2 cell; float4 uvRect; float4 fg; float4 bg;\n"
    @"  float2 glyphOrigin; float2 glyphSize; float flags;\n"
    @"  float pad0; float pad1; float pad2;\n"
    @"};\n"
    @"constant float kUnderline = 2.0;\n"
    @"constant float kStrike    = 4.0;\n"
    @"constant float kCursor    = 256.0;\n"
    @"constant float kCursorHollow = 512.0;\n"
    @"static inline bool has_flag(float flags, float bit) {\n"
    @"  return fmod(floor(flags / bit), 2.0) >= 0.5;\n"
    @"}\n"
    @"struct VOut {\n"
    @"  float4 pos [[position]]; float2 local; float4 fg; float4 bg;\n"
    @"  float4 uvRect; float2 glyphOrigin; float2 glyphSize; float flags;\n"
    @"};\n"
    @"vertex VOut vs(uint vid [[vertex_id]], uint iid [[instance_id]],\n"
    @"               constant Instance *insts [[buffer(0)]],\n"
    @"               constant Uniforms &u [[buffer(1)]]) {\n"
    @"  const float2 corners[6] = { float2(0,0), float2(1,0), float2(0,1),\n"
    @"                              float2(0,1), float2(1,0), float2(1,1) };\n"
    @"  float2 c = corners[vid];\n"
    @"  Instance it = insts[iid];\n"
    @"  float2 px = (it.cell + c) * u.cellPx;\n"
    @"  float2 ndc = float2(px.x / u.viewport.x * 2.0 - 1.0,\n"
    @"                      1.0 - px.y / u.viewport.y * 2.0);\n"
    @"  VOut o;\n"
    @"  o.pos = float4(ndc, 0.0, 1.0); o.local = c;\n"
    @"  o.fg = it.fg; o.bg = it.bg; o.uvRect = it.uvRect;\n"
    @"  o.glyphOrigin = it.glyphOrigin; o.glyphSize = it.glyphSize;\n"
    @"  o.flags = it.flags;\n"
    @"  return o;\n"
    @"}\n"
    @"fragment float4 fs(VOut in [[stage_in]],\n"
    @"                   texture2d<float> atlas [[texture(0)]],\n"
    @"                   constant Uniforms &u [[buffer(1)]]) {\n"
    @"  constexpr sampler s(address::clamp_to_edge, filter::linear);\n"
    @"  if (has_flag(in.flags, kCursorHollow)) {\n"
    @"    float t = 1.5;\n"
    @"    float2 px = in.local * u.cellPx;\n"
    @"    bool edge = px.x < t || px.y < t || (u.cellPx.x - px.x) < t || (u.cellPx.y - px.y) < t;\n"
    @"    if (edge) { return float4(in.fg.rgb, 1.0); }\n"
    @"    return float4(0.0, 0.0, 0.0, 0.0);\n"
    @"  }\n"
    @"  if (has_flag(in.flags, kCursor)) { return float4(in.fg.rgb, 0.75); }\n"
    @"  float2 p = in.local * u.cellPx;\n"
    @"  float alpha = 0.0;\n"
    @"  if (in.glyphSize.x > 0.0 && in.glyphSize.y > 0.0) {\n"
    @"    float2 g = (p - in.glyphOrigin) / in.glyphSize;\n"
    @"    if (g.x >= 0.0 && g.x <= 1.0 && g.y >= 0.0 && g.y <= 1.0) {\n"
    @"      alpha = atlas.sample(s, in.uvRect.xy + g * in.uvRect.zw).r;\n"
    @"    }\n"
    @"  }\n"
    @"  float4 color = mix(in.bg, float4(in.fg.rgb, 1.0), alpha);\n"
    @"  float thickness = max(1.0, u.cellPx.y / 16.0);\n"
    @"  if (has_flag(in.flags, kUnderline)) {\n"
    @"    float y0 = u.cellPx.y - thickness * 2.0;\n"
    @"    if (p.y >= y0 && p.y < y0 + thickness) color = float4(in.fg.rgb, 1.0);\n"
    @"  }\n"
    @"  if (has_flag(in.flags, kStrike)) {\n"
    @"    float y0 = u.cellPx.y * 0.5;\n"
    @"    if (p.y >= y0 && p.y < y0 + thickness) color = float4(in.fg.rgb, 1.0);\n"
    @"  }\n"
    @"  return color;\n"
    @"}\n";

// ---------------------------------------------------------------- structs

typedef struct {
    vector_float2 viewport;
    vector_float2 cellPx;
    uint32_t cols;
    uint32_t _pad[3];
} RillUniforms;

typedef struct {
    vector_float2 cell;
    vector_float4 uvRect;
    vector_float4 fg;
    vector_float4 bg;
    vector_float2 glyphOrigin;
    vector_float2 glyphSize;
    float flags;
    float _pad0, _pad1, _pad2;
} RillInstance;

#define RILL_FLAG_UNDERLINE 2u
#define RILL_FLAG_STRIKE    4u
#define RILL_FLAG_CURSOR    256u
#define RILL_FLAG_CURSOR_HOLLOW 512u

#define RILL_ATLAS_DIM 2048
#define RILL_MAX_FRAMES_IN_FLIGHT 1
#define RILL_MAX_DRAWABLES 2

typedef struct {
    float u, v, w, h;      /* normalized atlas rect */
    float originX, originY; /* pixels within the cell, top-left origin */
    float sizeX, sizeY;     /* pixels */
    BOOL valid;
} RillGlyph;

// ---------------------------------------------------------------- sentinel

typedef struct {
    BOOL armed;
    uint32_t codepoint;
    uint16_t col;
    uint16_t row;
    double keyTimestamp; /* CACurrentMediaTime timebase */
    double commitTime;
} RillSentinel;

@implementation TerminalView {
    RillClient *_client;

    id<MTLDevice> _device;
    id<MTLCommandQueue> _queue;
    id<MTLRenderPipelineState> _pipeline;
    id<MTLTexture> _atlas;
    id<MTLBuffer> _instanceBuffers[RILL_MAX_FRAMES_IN_FLIGHT];
    NSUInteger _frameIndex;
    dispatch_semaphore_t _inflight;

    NSMutableDictionary<NSNumber *, NSValue *> *_glyphCache;
    int _atlasPenX, _atlasPenY, _atlasShelfHeight;

    CTFontRef _font;
    CTFontRef _fontBold;
    CGFloat _cellW, _cellH, _ascent;
    uint16_t _cols, _rows;

    RillInstance *_instances;   /* persistent CPU mirror; damaged rows only */
    NSUInteger _instanceCount;

    dispatch_source_t _readSource;
    NSTimer *_pumpTimer;
    CADisplayLink *_displayLink;
    CFTimeInterval _targetPresentTime;

    /* T-NFR */
    RillSentinel _sentinel;
    NSMutableArray<NSNumber *> *_samples;
    NSMutableArray<NSNumber *> *_presentCadence;
    uint32_t _discards;
    uint32_t _nfrTarget;
    uint32_t _nfrSeq;
    RillNfrMode _nfrMode;
    BOOL _nfrRunning;
    BOOL _nfrFailed;
    uint32_t _nfrHidKeyDowns;
    BOOL _timerPump;

    NSUInteger _lastDrawCount;
    uint16_t _lastDrawCols;
    uint16_t _lastDrawRows;
    BOOL _sentinelInMirror;
    double _lastAnyPresented;
}

// ---------------------------------------------------------------- lifecycle

- (instancetype)initWithClient:(RillClient *)client {
    id<MTLDevice> device = MTLCreateSystemDefaultDevice();
    if (!device) {
        NSLog(@"Rill: no Metal device");
        return nil;
    }
    self = [super initWithFrame:NSMakeRect(0, 0, 800, 480) device:device];
    if (!self) {
        return nil;
    }
    _device = device;
    _client = client;
    _glyphCache = [NSMutableDictionary dictionary];
    _samples = [NSMutableArray array];
    _presentCadence = [NSMutableArray array];
    _frameIndex = 0;
    _inflight = dispatch_semaphore_create(RILL_MAX_FRAMES_IN_FLIGHT);

    if (![self setupFont]) {
        return nil;
    }
    if (![self setupMetal]) {
        return nil;
    }
    [self armSocketSource];
    return self;
}

- (void)dealloc {
    if (_readSource) {
        dispatch_source_cancel(_readSource);
    }
    if (_pumpTimer) {
        [_pumpTimer invalidate];
        _pumpTimer = nil;
    }
    if (_displayLink) {
        [_displayLink invalidate];
        _displayLink = nil;
    }
    if (_instances) {
        free(_instances);
    }
    if (_font) {
        CFRelease(_font);
    }
    if (_fontBold) {
        CFRelease(_fontBold);
    }
}

- (BOOL)setupFont {
    const char *family = rill_client_font_family(_client);
    CGFloat size = rill_client_font_size(_client);
    NSMutableArray<NSString *> *names = [NSMutableArray array];
    if (family && family[0]) {
        [names addObject:@(family)];
    }
    for (uint32_t i = 0;; i++) {
        const char *fb = rill_client_font_fallback(_client, i);
        if (!fb || !fb[0]) {
            break;
        }
        [names addObject:@(fb)];
    }
    _font = NULL;
    for (NSString *name in names) {
        CTFontRef candidate = CTFontCreateWithName((__bridge CFStringRef)name, size, NULL);
        if (candidate) {
            _font = candidate;
            break;
        }
    }
    if (!_font) {
        return NO;
    }
    CTFontSymbolicTraits bold = kCTFontTraitBold;
    _fontBold = CTFontCreateCopyWithSymbolicTraits(_font, size, NULL, bold, bold);
    if (!_fontBold) {
        _fontBold = (CTFontRef)CFRetain(_font);
    }

    /* Cell geometry from the font's own metrics, not a hardcoded 8x16
     * (SPEC-DISPLAY §5). */
    UniChar m = 'M';
    CGGlyph g = 0;
    CGSize advance = CGSizeZero;
    if (CTFontGetGlyphsForCharacters(_font, &m, &g, 1)) {
        CTFontGetAdvancesForGlyphs(_font, kCTFontOrientationHorizontal, &g, &advance, 1);
    }
    _cellW = ceil(advance.width > 0 ? advance.width : size * 0.6);
    _ascent = ceil(CTFontGetAscent(_font));
    _cellH = ceil(CTFontGetAscent(_font) + CTFontGetDescent(_font) + CTFontGetLeading(_font));
    if (_cellH < 1) {
        _cellH = ceil(size * 1.2);
    }
    return YES;
}

- (BOOL)setupMetal {
    /* ADR 0006: MTKView is the layer host only. drawInMTKView does not present. */
    self.colorPixelFormat = MTLPixelFormatBGRA8Unorm;
    self.clearColor = MTLClearColorMake(0.07, 0.07, 0.07, 1.0);
    self.framebufferOnly = YES;
    self.paused = YES;
    self.enableSetNeedsDisplay = NO;
    self.autoResizeDrawable = YES;
    self.preferredFramesPerSecond = 120;
    self.delegate = self;

    CAMetalLayer *layer = (CAMetalLayer *)self.layer;
    layer.opaque = YES;
    layer.maximumDrawableCount = RILL_MAX_DRAWABLES;
    layer.displaySyncEnabled = YES; /* the recorded number is what a person sees */
    layer.allowsNextDrawableTimeout = YES;
    layer.presentsWithTransaction = NO;

    _queue = [_device newCommandQueue];

    NSError *err = nil;
    id<MTLLibrary> lib = [_device newLibraryWithSource:kShaderSource options:nil error:&err];
    if (!lib) {
        NSLog(@"Rill: shader compile failed: %@", err);
        return NO;
    }
    MTLRenderPipelineDescriptor *desc = [MTLRenderPipelineDescriptor new];
    desc.vertexFunction = [lib newFunctionWithName:@"vs"];
    desc.fragmentFunction = [lib newFunctionWithName:@"fs"];
    desc.colorAttachments[0].pixelFormat = MTLPixelFormatBGRA8Unorm;
    desc.colorAttachments[0].blendingEnabled = YES;
    desc.colorAttachments[0].sourceRGBBlendFactor = MTLBlendFactorSourceAlpha;
    desc.colorAttachments[0].destinationRGBBlendFactor = MTLBlendFactorOneMinusSourceAlpha;
    desc.colorAttachments[0].sourceAlphaBlendFactor = MTLBlendFactorOne;
    desc.colorAttachments[0].destinationAlphaBlendFactor = MTLBlendFactorOneMinusSourceAlpha;
    _pipeline = [_device newRenderPipelineStateWithDescriptor:desc error:&err];
    if (!_pipeline) {
        NSLog(@"Rill: pipeline failed: %@", err);
        return NO;
    }

    MTLTextureDescriptor *td =
        [MTLTextureDescriptor texture2DDescriptorWithPixelFormat:MTLPixelFormatR8Unorm
                                                           width:RILL_ATLAS_DIM
                                                          height:RILL_ATLAS_DIM
                                                       mipmapped:NO];
    td.usage = MTLTextureUsageShaderRead;
    td.storageMode = MTLStorageModeManaged;
    _atlas = [_device newTextureWithDescriptor:td];
    _atlasPenX = 1;
    _atlasPenY = 1;
    _atlasShelfHeight = 0;
    return YES;
}

/* Event-driven: the socket wakes us, not a clock (ADR 0003 D2).
 * Negative control T-NFR / timer_pump restores the 60 Hz NSTimer. */
- (void)armSocketSource {
    const char *mut = getenv("RILL_MUTATE");
    if (mut && strcmp(mut, "timer_pump") == 0) {
        _timerPump = YES;
        _pumpTimer = [NSTimer scheduledTimerWithTimeInterval:(1.0 / 60.0)
                                                      target:self
                                                    selector:@selector(onSocketReadable)
                                                    userInfo:nil
                                                     repeats:YES];
        return;
    }
    int fd = rill_client_socket_fd(_client);
    if (fd < 0) {
        return;
    }
    _readSource = dispatch_source_create(DISPATCH_SOURCE_TYPE_READ, (uintptr_t)fd, 0,
                                         dispatch_get_main_queue());
    __weak TerminalView *weakSelf = self;
    dispatch_source_set_event_handler(_readSource, ^{
        [weakSelf onSocketReadable];
    });
    dispatch_resume(_readSource);
}

- (void)onSocketReadable {
    ptrdiff_t fed = rill_client_pump(_client);
    if (fed < 0) {
        NSLog(@"Rill: pump: %s", rill_client_last_error() ?: "error");
        return;
    }
    if (fed > 0) {
        [self renderFrame];
    }
}

- (void)viewDidMoveToWindow {
    [super viewDidMoveToWindow];
    self.paused = YES;
    if (self.window) {
        [self pinPresentRefresh];
        CAMetalLayer *layer = (CAMetalLayer *)self.layer;
        layer.opaque = YES;
        [self armDisplayLink];
    } else if (_displayLink) {
        _displayLink.paused = YES;
    }
}

- (void)pinPresentRefresh {
    NSInteger hz = 60;
    if (@available(macOS 12.0, *)) {
        NSScreen *screen = self.window.screen ?: NSScreen.mainScreen;
        NSInteger max = (NSInteger)screen.maximumFramesPerSecond;
        if (max > 0) {
            hz = max;
        }
    }
    self.preferredFramesPerSecond = hz;
    if (@available(macOS 14.0, *)) {
        if (_displayLink) {
            float f = (float)hz;
            _displayLink.preferredFrameRateRange = CAFrameRateRangeMake(f, f, f);
        }
    }
}

- (void)armDisplayLink {
    if (_displayLink) {
        _displayLink.paused = NO;
        return;
    }
    if (@available(macOS 14.0, *)) {
        _displayLink = [self displayLinkWithTarget:self selector:@selector(onDisplayLink:)];
        NSInteger hz = self.preferredFramesPerSecond > 0 ? self.preferredFramesPerSecond : 120;
        float f = (float)hz;
        _displayLink.preferredFrameRateRange = CAFrameRateRangeMake(f, f, f);
        [_displayLink addToRunLoop:[NSRunLoop mainRunLoop] forMode:NSRunLoopCommonModes];
    }
}

- (void)onDisplayLink:(CADisplayLink *)link {
    _targetPresentTime = link.targetTimestamp;
}

/* Same-stack echo: the key event is the wake. A second dispatch_source turn
 * is what made key_to_commit ~2.5ms. poll is bounded and only waits for the
 * attach socket, not a clock. */

- (void)drawInMTKView:(MTKView *)view {
    (void)view;
}

- (void)mtkView:(MTKView *)view drawableSizeWillChange:(CGSize)size {
    (void)view;
    (void)size;
}

// ---------------------------------------------------------------- atlas

- (RillGlyph)glyphForCodepoint:(uint32_t)cp bold:(BOOL)bold {
    NSNumber *key = @(((uint64_t)cp << 1) | (bold ? 1u : 0u));
    NSValue *cached = _glyphCache[key];
    if (cached) {
        RillGlyph g;
        [cached getValue:&g];
        return g;
    }

    RillGlyph out = {0};
    CTFontRef font = bold ? _fontBold : _font;

    UniChar uc[2];
    CFIndex n = 0;
    if (cp <= 0xFFFF) {
        uc[0] = (UniChar)cp;
        n = 1;
    } else {
        uint32_t v = cp - 0x10000;
        uc[0] = (UniChar)(0xD800 + (v >> 10));
        uc[1] = (UniChar)(0xDC00 + (v & 0x3FF));
        n = 2;
    }
    CGGlyph glyphs[2] = {0, 0};
    if (!CTFontGetGlyphsForCharacters(font, uc, glyphs, n) || glyphs[0] == 0) {
        /* Colour emoji and anything the family cannot render become an explicit
         * empty cell, counted by the caller. Silent mis-rendering is not
         * acceptable (ADR 0003 D1). */
        out.valid = YES;
        _glyphCache[key] = [NSValue valueWithBytes:&out objCType:@encode(RillGlyph)];
        return out;
    }

    CGRect bounds =
        CTFontGetBoundingRectsForGlyphs(font, kCTFontOrientationHorizontal, glyphs, NULL, 1);
    int w = (int)ceil(CGRectGetWidth(bounds)) + 2;
    int h = (int)ceil(CGRectGetHeight(bounds)) + 2;
    if (w <= 2 || h <= 2) {
        out.valid = YES; /* whitespace */
        _glyphCache[key] = [NSValue valueWithBytes:&out objCType:@encode(RillGlyph)];
        return out;
    }

    /* Shelf packing. On exhaustion we stop caching rather than corrupt the
     * atlas; a real LRU/repack is Milestone 1. */
    if (_atlasPenX + w + 1 >= RILL_ATLAS_DIM) {
        _atlasPenX = 1;
        _atlasPenY += _atlasShelfHeight + 1;
        _atlasShelfHeight = 0;
    }
    if (_atlasPenY + h + 1 >= RILL_ATLAS_DIM) {
        NSLog(@"Rill: glyph atlas full; U+%04X not cached", cp);
        out.valid = NO;
        return out;
    }

    size_t stride = (size_t)w;
    uint8_t *bitmap = calloc(stride * (size_t)h, 1);
    if (!bitmap) {
        out.valid = NO;
        return out;
    }
    CGColorSpaceRef gray = CGColorSpaceCreateDeviceGray();
    CGContextRef ctx = CGBitmapContextCreate(bitmap, (size_t)w, (size_t)h, 8, stride, gray,
                                             (CGBitmapInfo)kCGImageAlphaNone);
    CGColorSpaceRelease(gray);
    if (!ctx) {
        free(bitmap);
        out.valid = NO;
        return out;
    }
    CGContextSetShouldAntialias(ctx, true);
    CGContextSetShouldSmoothFonts(ctx, false); /* grayscale AA; R8 atlas */
    CGContextSetGrayFillColor(ctx, 1.0, 1.0);
    CGContextSetTextMatrix(ctx, CGAffineTransformIdentity);
    CGPoint at = CGPointMake(1 - bounds.origin.x, 1 - bounds.origin.y);
    CTFontDrawGlyphs(font, glyphs, &at, 1, ctx);
    CGContextRelease(ctx);

    [_atlas replaceRegion:MTLRegionMake2D((NSUInteger)_atlasPenX, (NSUInteger)_atlasPenY,
                                          (NSUInteger)w, (NSUInteger)h)
              mipmapLevel:0
                withBytes:bitmap
              bytesPerRow:stride];
    free(bitmap);

    out.u = (float)_atlasPenX / (float)RILL_ATLAS_DIM;
    out.v = (float)_atlasPenY / (float)RILL_ATLAS_DIM;
    out.w = (float)w / (float)RILL_ATLAS_DIM;
    out.h = (float)h / (float)RILL_ATLAS_DIM;
    out.sizeX = (float)w;
    out.sizeY = (float)h;
    /* Cell-local, top-left origin: baseline sits at _ascent from the top. */
    out.originX = (float)(bounds.origin.x - 1);
    out.originY = (float)(_ascent - (bounds.origin.y + CGRectGetHeight(bounds)) - 1);
    out.valid = YES;

    _atlasPenX += w + 1;
    if (h > _atlasShelfHeight) {
        _atlasShelfHeight = h;
    }
    _glyphCache[key] = [NSValue valueWithBytes:&out objCType:@encode(RillGlyph)];
    return out;
}

// ---------------------------------------------------------------- rendering

static inline vector_float4 rgba(uint32_t c) {
    return (vector_float4){((c >> 24) & 0xff) / 255.0f, ((c >> 16) & 0xff) / 255.0f,
                           ((c >> 8) & 0xff) / 255.0f, 1.0f};
}

- (void)ensureInstanceCapacityForCols:(uint16_t)cols rows:(uint16_t)rows {
    NSUInteger needed = (NSUInteger)cols * (NSUInteger)rows + 1; /* +1 cursor */
    if (_cols == cols && _rows == rows && _instances) {
        return;
    }
    _cols = cols;
    _rows = rows;
    free(_instances);
    _instances = calloc(needed, sizeof(RillInstance));
    _instanceCount = needed;
    size_t bytes = needed * sizeof(RillInstance);
    for (int i = 0; i < RILL_MAX_FRAMES_IN_FLIGHT; i++) {
        _instanceBuffers[i] = [_device newBufferWithLength:bytes
                                                   options:MTLResourceStorageModeShared];
    }
}

- (void)renderFrame {
    RillPodGrid grid = {0};
    if (rill_client_snapshot(_client, &grid) != 0 || grid.cells == NULL || grid.ncells == 0) {
        return;
    }
    [self ensureInstanceCapacityForCols:grid.cols rows:grid.rows];
    if (!_instances) {
        return;
    }

    /* Only damaged rows are rebuilt; the rest of the mirror persists across
     * frames (ADR 0003 D3). The old path re-rasterised every cell every frame
     * and ignored the damage range entirely. */
    uint16_t r0 = grid.full_damage ? 0 : grid.damage_row0;
    uint16_t r1 = grid.full_damage ? (grid.rows ? grid.rows - 1 : 0) : grid.damage_row1;
    if (r1 >= grid.rows) {
        r1 = grid.rows ? grid.rows - 1 : 0;
    }

    for (uint16_t y = r0; y <= r1 && y < grid.rows; y++) {
        for (uint16_t x = 0; x < grid.cols; x++) {
            size_t i = (size_t)y * grid.cols + x;
            if (i >= grid.ncells || i >= _instanceCount) {
                continue;
            }
            RillPodCell cell = grid.cells[i];
            BOOL bold = (cell.attrs & 1u) != 0;
            BOOL inverse = (cell.attrs & 4u) != 0;

            vector_float4 fg = rgba(cell.fg);
            vector_float4 bg = rgba(cell.bg);
            if (inverse) {
                vector_float4 t = fg;
                fg = bg;
                bg = t;
            }

            uint32_t cp = cell.codepoint ? cell.codepoint : 32;
            RillGlyph g = (cp <= 32) ? (RillGlyph){0} : [self glyphForCodepoint:cp bold:bold];

            float flags = 0.0f;
            if (cell.attrs & 2u) {
                flags += (float)RILL_FLAG_UNDERLINE;
            }

            RillInstance *inst = &_instances[i];
            inst->cell = (vector_float2){(float)x, (float)y};
            inst->uvRect = (vector_float4){g.u, g.v, g.w, g.h};
            inst->fg = fg;
            inst->bg = bg;
            inst->glyphOrigin = (vector_float2){g.originX, g.originY};
            inst->glyphSize = (vector_float2){g.sizeX, g.sizeY};
            inst->flags = flags;
        }
    }

    NSUInteger drawCount = (NSUInteger)grid.cols * (NSUInteger)grid.rows;
    if (grid.cursor_visible && drawCount < _instanceCount) {
        RillInstance *cur = &_instances[drawCount];
        memset(cur, 0, sizeof(*cur));
        cur->cell = (vector_float2){(float)grid.cursor_col, (float)grid.cursor_row};
        cur->fg = (vector_float4){0.85f, 0.85f, 0.85f, 1.0f};
        cur->flags = rill_client_alive(_client)
                         ? (float)RILL_FLAG_CURSOR
                         : (float)RILL_FLAG_CURSOR_HOLLOW;
        drawCount += 1;
    }

    _lastDrawCount = drawCount;
    _lastDrawCols = grid.cols;
    _lastDrawRows = grid.rows;
    _sentinelInMirror = NO;
    if (_sentinel.armed && grid.cells && grid.ncells > 0) {
        size_t idx = (size_t)_sentinel.row * grid.cols + _sentinel.col;
        if (idx < grid.ncells && grid.cells[idx].codepoint == _sentinel.codepoint) {
            _sentinelInMirror = YES;
        }
    }
    [self presentEcho];
    if (!rill_client_alive(_client) && self.window) {
        int st = rill_client_exit_status(_client);
        [self.window setTitle:[NSString stringWithFormat:@"Rill — exited %d", st]];
    }
}

- (void)presentEcho {
    CAMetalLayer *layer = (CAMetalLayer *)self.layer;
    if (!layer || _lastDrawCount == 0) {
        return;
    }
    layer.opaque = YES;
    CGFloat scale = self.window.backingScaleFactor > 0 ? self.window.backingScaleFactor : 1.0;
    if (layer.contentsScale != scale) {
        layer.contentsScale = scale;
    }
    CGSize backing = [self convertSizeToBacking:self.bounds.size];
    if (backing.width >= 1 && backing.height >= 1 &&
        !CGSizeEqualToSize(layer.drawableSize, backing)) {
        layer.drawableSize = backing;
        self.drawableSize = backing;
    }
    dispatch_semaphore_wait(_inflight, DISPATCH_TIME_FOREVER);
    id<CAMetalDrawable> drawable = [layer nextDrawable];
    if (!drawable) {
        dispatch_semaphore_signal(_inflight);
        return;
    }
    [self presentOnDrawable:drawable];
}

- (void)noteDrawablePresented:(double)presented {
    if (!_nfrRunning || presented <= 0) {
        return;
    }
    if (_lastAnyPresented > 0) {
        double dt = (presented - _lastAnyPresented) * 1000.0;
        if (dt > 0 && dt < 500) {
            [_presentCadence addObject:@(dt)];
        }
    }
    _lastAnyPresented = presented;
}

- (void)presentOnDrawable:(id<CAMetalDrawable>)drawable {
    NSUInteger count = _lastDrawCount;
    uint16_t cols = _lastDrawCols;
    BOOL carriesSentinel = _sentinelInMirror;
    if (!drawable || count == 0 || !_instances || !_instanceBuffers[0]) {
        dispatch_semaphore_signal(_inflight);
        return;
    }

    CGFloat scale = self.window.backingScaleFactor > 0 ? self.window.backingScaleFactor : 1.0;
    CGSize drawableSize = CGSizeMake(drawable.texture.width, drawable.texture.height);
    if (drawableSize.width < 1 || drawableSize.height < 1) {
        dispatch_semaphore_signal(_inflight);
        return;
    }

    id<MTLBuffer> buf = _instanceBuffers[_frameIndex % RILL_MAX_FRAMES_IN_FLIGHT];
    _frameIndex++;
    memcpy(buf.contents, _instances, count * sizeof(RillInstance));

    RillUniforms u;
    u.viewport = (vector_float2){(float)drawableSize.width, (float)drawableSize.height};
    u.cellPx = (vector_float2){(float)(_cellW * scale), (float)(_cellH * scale)};
    u.cols = cols;

    MTLRenderPassDescriptor *pass = [MTLRenderPassDescriptor renderPassDescriptor];
    pass.colorAttachments[0].texture = drawable.texture;
    pass.colorAttachments[0].loadAction = MTLLoadActionClear;
    pass.colorAttachments[0].clearColor = MTLClearColorMake(0.07, 0.07, 0.07, 1.0);
    pass.colorAttachments[0].storeAction = MTLStoreActionStore;

    id<MTLCommandBuffer> cmd = [_queue commandBuffer];
    id<MTLRenderCommandEncoder> enc = [cmd renderCommandEncoderWithDescriptor:pass];
    [enc setRenderPipelineState:_pipeline];
    [enc setVertexBuffer:buf offset:0 atIndex:0];
    [enc setVertexBytes:&u length:sizeof(u) atIndex:1];
    [enc setFragmentBytes:&u length:sizeof(u) atIndex:1];
    [enc setFragmentTexture:_atlas atIndex:0];
    [enc drawPrimitives:MTLPrimitiveTypeTriangle
            vertexStart:0
            vertexCount:6
          instanceCount:count];
    [enc endEncoding];

    double keyTs = 0;
    double commitTs = CACurrentMediaTime();
    BOOL sample = carriesSentinel && _sentinel.armed;
    if (sample) {
        keyTs = _sentinel.keyTimestamp;
        _sentinel.commitTime = commitTs;
        _sentinel.armed = NO;
        _sentinelInMirror = NO;
    }
    __weak TerminalView *weakSelf = self;
    [drawable addPresentedHandler:^(id<MTLDrawable> d) {
        double presented = d.presentedTime;
        TerminalView *s = weakSelf;
        if (s) {
            dispatch_semaphore_signal(s->_inflight);
        }
        dispatch_async(dispatch_get_main_queue(), ^{
            TerminalView *view = weakSelf;
            if (!view) {
                return;
            }
            double prev = view->_lastAnyPresented;
            [view noteDrawablePresented:presented];
            if (sample) {
                if (view->_samples.count < 8 && keyTs > 0) {
                    double delta = prev > 0 ? (presented - prev) * 1000.0 : 0;
                    fprintf(stderr,
                            "T-NFR seg n=%lu key_to_commit=%.2fms "
                            "commit_to_presented=%.2fms present_delta=%.2fms total=%.2fms\n",
                            (unsigned long)view->_samples.count + 1,
                            (commitTs - keyTs) * 1000.0,
                            (presented - commitTs) * 1000.0,
                            delta,
                            (presented - keyTs) * 1000.0);
                    fflush(stderr);
                }
                [view sentinelPresentedAt:presented forKeyAt:keyTs];
            }
        });
    }];
    CFTimeInterval at = _targetPresentTime;
    if (at > CACurrentMediaTime() + 0.0002) {
        [cmd presentDrawable:drawable atTime:at];
    } else {
        [cmd presentDrawable:drawable];
    }
    [cmd commit];
}

- (void)setFrameSize:(NSSize)newSize {
    [super setFrameSize:newSize];
    if (_cellW < 1 || _cellH < 1) {
        return;
    }
    uint16_t cols = (uint16_t)MAX(20, (int)(newSize.width / _cellW));
    uint16_t rows = (uint16_t)MAX(8, (int)(newSize.height / _cellH));
    rill_client_resize(_client, cols, rows, (uint16_t)newSize.width, (uint16_t)newSize.height);
    [self renderFrame];
}

// ---------------------------------------------------------------- input

- (BOOL)acceptsFirstResponder {
    return YES;
}

- (void)sendBytes:(const uint8_t *)bytes length:(size_t)len {
    if (len == 0) {
        return;
    }
    if (!rill_client_alive(_client)) {
        return;
    }
    if (rill_client_send_input(_client, bytes, len) != 0) {
        NSLog(@"Rill: send_input: %s", rill_client_last_error() ?: "error");
        return;
    }
    [self paintEchoAfterInput];
}

- (void)paintEchoAfterInput {
    if (_timerPump) {
        return;
    }
    int fd = rill_client_socket_fd(_client);
    CFTimeInterval deadline = CACurrentMediaTime() + 0.002;
    for (;;) {
        ptrdiff_t fed = rill_client_pump(_client);
        if (fed < 0) {
            return;
        }
        if (fed > 0) {
            [self renderFrame];
            return;
        }
        CFTimeInterval left = deadline - CACurrentMediaTime();
        if (left <= 0 || fd < 0) {
            return;
        }
        struct pollfd pfd = {.fd = fd, .events = POLLIN, .revents = 0};
        int ms = (int)ceil(left * 1000.0);
        if (ms < 1) {
            ms = 0;
        }
        if (poll(&pfd, 1, ms) <= 0) {
            return;
        }
    }
}

/* ^C was untypeable in the previous build, which also made T-DROP unreachable
 * through the GUI (docs/SPIKE-0-AUDIT.md S3-8g). */
- (void)keyDown:(NSEvent *)event {
    if (_nfrRunning && _nfrMode == RillNfrModeHid) {
        _nfrHidKeyDowns++;
        if (_nfrHidKeyDowns == 1) {
            fprintf(stderr, "T-NFR hid keyDown characters=%s keyCode=%u\n",
                    event.characters.UTF8String ?: "", (unsigned)event.keyCode);
            fflush(stderr);
        }
    }
    if (_nfrRunning && _sentinel.armed && _sentinel.keyTimestamp == 0 && event.characters.length) {
        unichar typed = [event.characters characterAtIndex:0];
        if ((uint32_t)typed == _sentinel.codepoint) {
            /* Map NSEvent.timestamp onto the presentedTime timebase
             * (CACurrentMediaTime). Mixing boot-uptime with mach time produced
             * negative intervals and a 100% discard run. */
            NSTimeInterval uptime = NSProcessInfo.processInfo.systemUptime;
            NSTimeInterval ca = CACurrentMediaTime();
            _sentinel.keyTimestamp = event.timestamp + (ca - uptime);
        }
    }
    NSEventModifierFlags mods = event.modifierFlags;
    NSString *plain = event.charactersIgnoringModifiers;

    switch (event.keyCode) {
        case 36: { uint8_t b = '\r'; [self sendBytes:&b length:1]; return; }
        case 51: { uint8_t b = 0x7f; [self sendBytes:&b length:1]; return; }
        case 53: { uint8_t b = 0x1b; [self sendBytes:&b length:1]; return; }
        case 48: { uint8_t b = '\t'; [self sendBytes:&b length:1]; return; }
        case 126: { const uint8_t s[] = {0x1b, '[', 'A'}; [self sendBytes:s length:3]; return; }
        case 125: { const uint8_t s[] = {0x1b, '[', 'B'}; [self sendBytes:s length:3]; return; }
        case 124: { const uint8_t s[] = {0x1b, '[', 'C'}; [self sendBytes:s length:3]; return; }
        case 123: { const uint8_t s[] = {0x1b, '[', 'D'}; [self sendBytes:s length:3]; return; }
        default: break;
    }

    if ((mods & NSEventModifierFlagControl) && plain.length == 1) {
        unichar c = [plain characterAtIndex:0];
        uint8_t b = 0;
        if (c >= 'a' && c <= 'z') {
            b = (uint8_t)(c - 'a' + 1);
        } else if (c >= 'A' && c <= 'Z') {
            b = (uint8_t)(c - 'A' + 1);
        } else if (c == '[') {
            b = 0x1b;
        } else if (c == '\\') {
            b = 0x1c;
        } else if (c == ']') {
            b = 0x1d;
        } else if (c == ' ' || c == '@') {
            b = 0x00;
        }
        if (b || c == ' ' || c == '@') {
            [self sendBytes:&b length:1];
            return;
        }
    }

    NSString *chars = event.characters;
    if (chars.length == 0) {
        return;
    }
    if (mods & NSEventModifierFlagOption) {
        uint8_t esc = 0x1b;
        [self sendBytes:&esc length:1];
    }
    NSData *data = [chars dataUsingEncoding:NSUTF8StringEncoding];
    [self sendBytes:data.bytes length:data.length];
}

/* NSTextInputClient: minimal conformance so the responder chain is correct.
 * Full IME (marked text rendered in-cell, candidate positioning) is
 * SPEC-DISPLAY §6 and is NOT implemented — recorded here rather than left to
 * look finished. */
- (void)insertText:(id)string replacementRange:(NSRange)r {
    (void)r;
    NSString *s = [string isKindOfClass:[NSAttributedString class]] ? [string string] : string;
    if (_nfrRunning && _sentinel.armed && _sentinel.keyTimestamp == 0 && s.length) {
        unichar typed = [s characterAtIndex:0];
        if ((uint32_t)typed == _sentinel.codepoint) {
            _sentinel.keyTimestamp = CACurrentMediaTime();
        }
    }
    NSData *d = [s dataUsingEncoding:NSUTF8StringEncoding];
    [self sendBytes:d.bytes length:d.length];
}
- (void)doCommandBySelector:(SEL)s { (void)s; }
- (void)setMarkedText:(id)t selectedRange:(NSRange)a replacementRange:(NSRange)b {
    (void)t; (void)a; (void)b;
}
- (void)unmarkText {}
- (NSRange)selectedRange { return NSMakeRange(NSNotFound, 0); }
- (NSRange)markedRange { return NSMakeRange(NSNotFound, 0); }
- (BOOL)hasMarkedText { return NO; }
- (NSAttributedString *)attributedSubstringForProposedRange:(NSRange)r actualRange:(NSRangePointer)a {
    (void)r; (void)a; return nil;
}
- (NSArray<NSAttributedStringKey> *)validAttributesForMarkedText { return @[]; }
- (NSRect)firstRectForCharacterRange:(NSRange)r actualRange:(NSRangePointer)a {
    (void)r; (void)a; return NSZeroRect;
}
- (NSUInteger)characterIndexForPoint:(NSPoint)p { (void)p; return NSNotFound; }

// ---------------------------------------------------------------- T-NFR

- (void)sentinelPresentedAt:(double)presented forKeyAt:(double)keyTs {
    if (!_nfrRunning) {
        return;
    }
    double ms = (presented - keyTs) * 1000.0;
    if (keyTs <= 0 || ms <= 0 || ms > 5000) {
        _discards++; /* no keyDown, clock skew, or a lost attribution */
    } else {
        [_samples addObject:@(ms)];
        if (_samples.count % 100 == 0) {
            fprintf(stderr, "T-NFR progress samples=%lu discarded=%u\n",
                    (unsigned long)_samples.count, _discards);
            fflush(stderr);
        }
    }
    [self injectNextSampleIfNeeded];
}

- (void)injectNextSampleIfNeeded {
    if (!_nfrRunning) {
        return;
    }
    if (_samples.count >= _nfrTarget) {
        _nfrRunning = NO;
        return;
    }
    /* Fail-fast: HID never reached AppKit. */
    if (_samples.count == 0 && _discards >= 20) {
        fprintf(stderr,
                "T-NFR: 20 HID keys posted, 0 presented, keyDown=%u. "
                "Stopping instead of waiting out the 180s deadline.\n",
                _nfrHidKeyDowns);
        fflush(stderr);
        _nfrFailed = YES;
        _nfrRunning = NO;
        return;
    }
    /* At most 2% discards of a 1000-accept run (ADR 0003 D6). */
    if (_discards > 20) {
        fprintf(stderr, "T-NFR: %u discards before 1000 accepts (cap 20).\n", _discards);
        fflush(stderr);
        _nfrFailed = YES;
        _nfrRunning = NO;
        return;
    }

    uint16_t col = 0, row = 0;
    if (rill_client_cursor(_client, &col, &row) != 0) {
        _nfrFailed = YES;
        _nfrRunning = NO;
        return;
    }
    /* A wrapping line is a discard under D6. Kill the line before the
     * cursor reaches the last cells so the oracle stays on one row. */
    if (_cols > 4 && col + 2 >= _cols) {
        uint8_t u = 0x15; /* Ctrl-U: DATA, not a control RPC */
        [self sendBytes:&u length:1];
        for (int i = 0; i < 40; i++) {
            rill_client_pump(_client);
            uint16_t c2 = 0, r2 = 0;
            if (rill_client_cursor(_client, &c2, &r2) == 0 && c2 + 2 < _cols) {
                col = c2;
                row = r2;
                break;
            }
            [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode
                                     beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.002]];
        }
        if (rill_client_cursor(_client, &col, &row) != 0) {
            _nfrFailed = YES;
            _nfrRunning = NO;
            return;
        }
    }
    uint32_t existing = rill_client_cell_codepoint(_client, col, row);

    /* A sentinel that could already be on screen is what made the old gate
     * unable to fail. Pick a printable codepoint the target cell does not
     * already hold (ADR 0003 D6). */
    uint32_t cp = 'a';
    for (uint32_t candidate = 'a'; candidate <= 'z'; candidate++) {
        if (candidate != existing) {
            cp = candidate;
            break;
        }
    }

    _nfrSeq++;
    uint32_t seq = _nfrSeq;
    _sentinel.armed = YES;
    _sentinel.codepoint = cp;
    _sentinel.col = col;
    _sentinel.row = row;
    _sentinel.keyTimestamp = 0;
    _sentinelInMirror = NO;

    [self injectCodepoint:cp];

    /* If nothing presents within 500ms the sample is discarded, not silently
     * dropped: discards above 2% fail the run (ADR 0003 D6). */
    __weak TerminalView *weakSelf = self;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.5 * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), ^{
                       TerminalView *s = weakSelf;
                       if (s && s->_nfrRunning && s->_nfrSeq == seq && s->_sentinel.armed) {
                           s->_sentinel.armed = NO;
                           s->_discards++;
                           if (s->_discards % 10 == 0) {
                               fprintf(stderr, "T-NFR progress samples=%lu discarded=%u\n",
                                       (unsigned long)s->_samples.count, s->_discards);
                               fflush(stderr);
                           }
                           [s injectNextSampleIfNeeded];
                       }
                   });
}

- (void)injectCodepoint:(uint32_t)cp {
    NSString *s = [NSString stringWithFormat:@"%C", (unichar)cp];

    if (_nfrMode == RillNfrModeHid) {
        CGKeyCode vk = rill_vk_ansi_letter(cp);
        CGEventSourceRef src = CGEventSourceCreate(kCGEventSourceStateHIDSystemState);
        CGEventRef down = CGEventCreateKeyboardEvent(src, vk, true);
        CGEventRef up = CGEventCreateKeyboardEvent(src, vk, false);
        if (src) {
            CFRelease(src);
        }
        if (!down || !up) {
            _nfrFailed = YES;
            _nfrRunning = NO;
            if (down) CFRelease(down);
            if (up) CFRelease(up);
            return;
        }
        UniChar u = (UniChar)cp;
        CGEventKeyboardSetUnicodeString(down, 1, &u);
        CGEventKeyboardSetUnicodeString(up, 1, &u);
        /* Session tap only (ADR 0003 D7). PostToPid in the same shot typed
         * twice, wrapped the shell line, and discarded the late samples. */
        [NSApp activateIgnoringOtherApps:YES];
        [self.window makeKeyAndOrderFront:nil];
        [self.window makeFirstResponder:self];
        CGEventPost(kCGSessionEventTap, down);
        CFRelease(down);
        NSDate *until = [NSDate dateWithTimeIntervalSinceNow:0.002];
        while (_sentinel.keyTimestamp == 0 && [until timeIntervalSinceNow] > 0) {
            NSEvent *queued = [NSApp nextEventMatchingMask:NSEventMaskAny
                                                 untilDate:until
                                                    inMode:NSDefaultRunLoopMode
                                                   dequeue:YES];
            if (queued) {
                [NSApp sendEvent:queued];
            }
        }
        CGEventPost(kCGSessionEventTap, up);
        CFRelease(up);
        return;
    }

    NSEvent *ev = [NSEvent keyEventWithType:NSEventTypeKeyDown
                                   location:NSZeroPoint
                              modifierFlags:0
                                  timestamp:CACurrentMediaTime()
                               windowNumber:self.window.windowNumber
                                    context:nil
                                 characters:s
                charactersIgnoringModifiers:s
                                  isARepeat:NO
                                    keyCode:0];
    _sentinel.keyTimestamp = ev.timestamp;
    [NSApp sendEvent:ev];
}

- (RillNfrReport)runNfrKeyWithMode:(RillNfrMode)mode count:(uint32_t)count {
    RillNfrReport report = {0};
    report.mode = (int)mode;
    report.vsync = 1;

    CAMetalLayer *layer = (CAMetalLayer *)self.layer;
    report.refresh_hz = 0;
    if (@available(macOS 12.0, *)) {
        NSScreen *screen = self.window.screen ?: NSScreen.mainScreen;
        report.refresh_hz = screen.maximumFramesPerSecond;
    }
    (void)layer;

    [_samples removeAllObjects];
    [_presentCadence removeAllObjects];
    _lastAnyPresented = 0;
    _discards = 0;
    _nfrTarget = count;
    _nfrSeq = 0;
    _nfrMode = mode;
    _nfrRunning = YES;
    _nfrFailed = NO;
    _nfrHidKeyDowns = 0;

    if (mode == RillNfrModeHid) {
        [NSApp activateIgnoringOtherApps:YES];
        [self.window makeKeyAndOrderFront:nil];
        [self.window makeFirstResponder:self];
    }

    rill_client_begin_warm_path_audit(_client);

    CAMetalLayer *metal = (CAMetalLayer *)self.layer;
    fprintf(stderr,
            "present: toggleFullScreen + opaque echo + same-stack pump "
            "fullscreen=%d opaque=%d drawable=%.0fx%.0f timer_pump=%d\n",
            (self.window.styleMask & NSWindowStyleMaskFullScreen) ? 1 : 0,
            metal.opaque ? 1 : 0, metal.drawableSize.width, metal.drawableSize.height,
            _timerPump ? 1 : 0);
    fflush(stderr);
    [self pinPresentRefresh];
    self.paused = YES;
    [NSCursor hide];

    /* Let the shell settle, then serialize ~36 vsync presents so ProMotion is
     * at 120 Hz before the first HID sample (early segs were 16.67ms). */
    for (int i = 0; i < 20; i++) {
        rill_client_pump(_client);
    }
    for (int i = 0; i < 36; i++) {
        [self renderFrame];
    }

    [self injectNextSampleIfNeeded];

    fprintf(stderr, "T-NFR measuring mode=%s count=%u\n",
            mode == RillNfrModeHid ? "hid" : "app", count);
    fflush(stderr);

    NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:180];
    while (_nfrRunning && [deadline timeIntervalSinceNow] > 0) {
        NSEvent *e = [NSApp nextEventMatchingMask:NSEventMaskAny
                                        untilDate:[NSDate dateWithTimeIntervalSinceNow:0.0005]
                                           inMode:NSDefaultRunLoopMode
                                          dequeue:YES];
        if (e) {
            [NSApp sendEvent:e];
        }
        if (_timerPump) {
            continue;
        }
        ptrdiff_t fed = rill_client_pump(_client);
        if (fed > 0) {
            [self renderFrame];
        }
    }
    _nfrRunning = NO;
    [NSCursor unhide];

    report.warm_path_violations = rill_client_end_warm_path_audit(_client);
    report.samples = (uint32_t)_samples.count;
    report.discarded = _discards;

    if (_presentCadence.count > 0) {
        NSArray<NSNumber *> *cadence =
            [_presentCadence sortedArrayUsingSelector:@selector(compare:)];
        NSUInteger cn = cadence.count;
        double p50 = cadence[(NSUInteger)((cn - 1) * 0.50)].doubleValue;
        double p95 = cadence[(NSUInteger)((cn - 1) * 0.95)].doubleValue;
        fprintf(stderr, "T-NFR present_cadence p50=%.2fms (~%.0fHz) p95=%.2fms n=%lu\n",
                p50, p50 > 0 ? 1000.0 / p50 : 0, p95, (unsigned long)cn);
        fflush(stderr);
    }

    if (_nfrFailed || _samples.count == 0) {
        report.ok = 0;
        return report;
    }

    NSArray<NSNumber *> *sorted =
        [_samples sortedArrayUsingSelector:@selector(compare:)];
    NSUInteger n = sorted.count;
    report.p50_ms = sorted[(NSUInteger)((n - 1) * 0.50)].doubleValue;
    report.p95_ms = sorted[(NSUInteger)((n - 1) * 0.95)].doubleValue;
    report.p99_ms = sorted[(NSUInteger)((n - 1) * 0.99)].doubleValue;
    report.max_ms = sorted[n - 1].doubleValue;
    report.ok = 1;
    return report;
}

@end
