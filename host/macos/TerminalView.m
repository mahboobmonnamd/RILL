#import "TerminalView.h"
#import <CoreText/CoreText.h>
#import <MetalKit/MetalKit.h>

static NSString *kShader = @"#include <metal_stdlib>\n"
                            "using namespace metal;\n"
                            "struct VOut { float4 pos [[position]]; float2 uv; };\n"
                            "vertex VOut vs(uint vid [[vertex_id]]) {\n"
                            "  float2 p[6] = { float2(-1,-1), float2(1,-1), float2(-1,1), float2(-1,1), float2(1,-1), float2(1,1) };\n"
                            "  float2 u[6] = { float2(0,1), float2(1,1), float2(0,0), float2(0,0), float2(1,1), float2(1,0) };\n"
                            "  VOut o; o.pos = float4(p[vid], 0, 1); o.uv = u[vid]; return o;\n"
                            "}\n"
                            "fragment float4 fs(VOut in [[stage_in]], texture2d<float> tex [[texture(0)]]) {\n"
                            "  constexpr sampler s(address::clamp_to_edge, filter::linear);\n"
                            "  return tex.sample(s, in.uv);\n"
                            "}\n";

@implementation TerminalView {
    RillClient *_client;
    id<MTLDevice> _device;
    id<MTLCommandQueue> _queue;
    id<MTLRenderPipelineState> _pipeline;
    id<MTLTexture> _texture;
    CTFontRef _font;
    NSTimer *_timer;
    uint16_t _cellW;
    uint16_t _cellH;
}

- (instancetype)initWithClient:(RillClient *)client {
    self = [super initWithFrame:NSMakeRect(0, 0, 800, 480)];
    if (!self) {
        return nil;
    }
    _client = client;
    _cellW = 8;
    _cellH = 16;
    self.wantsLayer = YES;
    CAMetalLayer *layer = [CAMetalLayer layer];
    _device = MTLCreateSystemDefaultDevice();
    layer.device = _device;
    layer.pixelFormat = MTLPixelFormatBGRA8Unorm;
    layer.framebufferOnly = NO;
    self.layer = layer;
    _queue = [_device newCommandQueue];
    NSError *err = nil;
    id<MTLLibrary> lib = [_device newLibraryWithSource:kShader options:nil error:&err];
    MTLRenderPipelineDescriptor *desc = [MTLRenderPipelineDescriptor new];
    desc.vertexFunction = [lib newFunctionWithName:@"vs"];
    desc.fragmentFunction = [lib newFunctionWithName:@"fs"];
    desc.colorAttachments[0].pixelFormat = MTLPixelFormatBGRA8Unorm;
    _pipeline = [_device newRenderPipelineStateWithDescriptor:desc error:&err];
    const char *family = rill_client_font_family(client);
    NSString *name = family ? [NSString stringWithUTF8String:family] : @"Menlo";
    CGFloat size = rill_client_font_size(client);
    _font = CTFontCreateWithName((__bridge CFStringRef)name, size, NULL);
    _timer = [NSTimer scheduledTimerWithTimeInterval:1.0 / 60.0
                                              target:self
                                            selector:@selector(pump)
                                            userInfo:nil
                                             repeats:YES];
    return self;
}

- (BOOL)acceptsFirstResponder {
    return YES;
}

- (void)keyDown:(NSEvent *)event {
    NSString *chars = event.charactersIgnoringModifiers;
    if (event.keyCode == 36) {
        uint8_t b = '\r';
        rill_client_send_input(_client, &b, 1);
        return;
    }
    if (event.keyCode == 51) {
        uint8_t b = 0x7f;
        rill_client_send_input(_client, &b, 1);
        return;
    }
    if (chars.length == 0) {
        return;
    }
    NSData *data = [chars dataUsingEncoding:NSUTF8StringEncoding];
    rill_client_send_input(_client, data.bytes, data.length);
}

- (void)setFrameSize:(NSSize)newSize {
    [super setFrameSize:newSize];
    uint16_t cols = (uint16_t)MAX(20, (int)(newSize.width / _cellW));
    uint16_t rows = (uint16_t)MAX(8, (int)(newSize.height / _cellH));
    rill_client_resize(_client, cols, rows, (uint16_t)newSize.width, (uint16_t)newSize.height);
}

- (void)pump {
    rill_client_pump(_client);
    RillPodGrid grid = {0};
    if (rill_client_snapshot(_client, &grid) != 0 || grid.cells == NULL || grid.ncells == 0) {
        return;
    }
    [self paintGrid:&grid];
}

- (void)paintGrid:(RillPodGrid *)grid {
    int w = (int)grid->cols * _cellW;
    int h = (int)grid->rows * _cellH;
    if (w <= 0 || h <= 0) {
        return;
    }
    CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    CGContextRef ctx = CGBitmapContextCreate(
        NULL, w, h, 8, w * 4, space, kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little);
    CGColorSpaceRelease(space);
    if (!ctx) {
        return;
    }
    CGContextSetTextMatrix(ctx, CGAffineTransformIdentity);
    for (uint16_t y = 0; y < grid->rows; y++) {
        for (uint16_t x = 0; x < grid->cols; x++) {
            size_t i = (size_t)y * grid->cols + x;
            if (i >= grid->ncells) {
                continue;
            }
            RillPodCell cell = grid->cells[i];
            float bgr = ((cell.bg >> 24) & 0xff) / 255.0f;
            float bgg = ((cell.bg >> 16) & 0xff) / 255.0f;
            float bgb = ((cell.bg >> 8) & 0xff) / 255.0f;
            CGContextSetRGBFillColor(ctx, bgr, bgg, bgb, 1);
            CGContextFillRect(ctx, CGRectMake(x * _cellW, (grid->rows - 1 - y) * _cellH, _cellW, _cellH));
            UTF32Char cp = cell.codepoint ? cell.codepoint : 32;
            if (cp < 32) {
                continue;
            }
            UniChar uc[2];
            UniCharCount n = 0;
            if (cp <= 0xffff) {
                uc[0] = (UniChar)cp;
                n = 1;
            } else {
                cp -= 0x10000;
                uc[0] = (UniChar)(0xd800 + (cp >> 10));
                uc[1] = (UniChar)(0xdc00 + (cp & 0x3ff));
                n = 2;
            }
            CGGlyph glyphs[2];
            if (!CTFontGetGlyphsForCharacters(_font, uc, glyphs, n)) {
                continue;
            }
            float fgr = ((cell.fg >> 24) & 0xff) / 255.0f;
            float fgg = ((cell.fg >> 16) & 0xff) / 255.0f;
            float fgb = ((cell.fg >> 8) & 0xff) / 255.0f;
            CGContextSetRGBFillColor(ctx, fgr, fgg, fgb, 1);
            CGPoint pos = CGPointMake(x * _cellW, (grid->rows - 1 - y) * _cellH + 3);
            CTFontDrawGlyphs(_font, glyphs, &pos, 1, ctx);
        }
    }
    if (grid->cursor_visible) {
        CGContextSetRGBFillColor(ctx, 0.8, 0.8, 0.8, 0.8);
        CGContextFillRect(
            ctx,
            CGRectMake(grid->cursor_col * _cellW, (grid->rows - 1 - grid->cursor_row) * _cellH, 2, _cellH));
    }
    uint8_t *src = CGBitmapContextGetData(ctx);
    MTLTextureDescriptor *td = [MTLTextureDescriptor
        texture2DDescriptorWithPixelFormat:MTLPixelFormatBGRA8Unorm
                                     width:w
                                    height:h
                                 mipmapped:NO];
    td.usage = MTLTextureUsageShaderRead;
    _texture = [_device newTextureWithDescriptor:td];
    [_texture replaceRegion:MTLRegionMake2D(0, 0, w, h)
                mipmapLevel:0
                  withBytes:src
                bytesPerRow:w * 4];
    CGContextRelease(ctx);

    CAMetalLayer *layer = (CAMetalLayer *)self.layer;
    layer.drawableSize = CGSizeMake(w, h);
    id<CAMetalDrawable> drawable = [layer nextDrawable];
    if (!drawable) {
        return;
    }
    MTLRenderPassDescriptor *pass = [MTLRenderPassDescriptor renderPassDescriptor];
    pass.colorAttachments[0].texture = drawable.texture;
    pass.colorAttachments[0].loadAction = MTLLoadActionClear;
    pass.colorAttachments[0].clearColor = MTLClearColorMake(0.07, 0.07, 0.07, 1);
    pass.colorAttachments[0].storeAction = MTLStoreActionStore;
    id<MTLCommandBuffer> cmd = [_queue commandBuffer];
    id<MTLRenderCommandEncoder> enc = [cmd renderCommandEncoderWithDescriptor:pass];
    [enc setRenderPipelineState:_pipeline];
    [enc setFragmentTexture:_texture atIndex:0];
    [enc drawPrimitives:MTLPrimitiveTypeTriangle vertexStart:0 vertexCount:6];
    [enc endEncoding];
    [cmd presentDrawable:drawable];
    [cmd commit];
}

- (void)dealloc {
    [_timer invalidate];
    if (_font) {
        CFRelease(_font);
    }
}

@end
