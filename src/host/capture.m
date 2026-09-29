#import <CoreGraphics/CoreGraphics.h>
#import <CoreMedia/CoreMedia.h>
#import <CoreVideo/CoreVideo.h>
#import <ImageIO/ImageIO.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>

typedef void (*PinholeFrameFn)(void *, const uint8_t *, uint32_t, uint32_t, uint32_t);
typedef void (*PinholeErrorFn)(void *, const char *);

static void pinhole_report(id cap, NSError *error, const char *fallback);

@interface PinholeCapture : NSObject <SCStreamOutput, SCStreamDelegate>
@property(nonatomic, assign) void *ctx;
@property(nonatomic, assign) PinholeFrameFn onFrame;
@property(nonatomic, assign) PinholeErrorFn onError;
@property(nonatomic, strong) NSLock *lock;
@property(nonatomic, strong) dispatch_queue_t queue;
@property(nonatomic, strong) SCStream *stream;
@property(nonatomic, assign) BOOL stopped;
- (void)begin;
- (void)shutdownAndWait;
@end

static void pinhole_report(PinholeCapture *cap, NSError *error, const char *fallback) {
    [cap.lock lock];
    BOOL stopped = cap.stopped;
    void *ctx = cap.ctx;
    PinholeErrorFn onError = cap.onError;
    [cap.lock unlock];
    if (stopped || !ctx || !onError) return;
    const char *message = error.localizedDescription.UTF8String;
    onError(ctx, message ? message : fallback);
}

@implementation PinholeCapture
- (void)begin {
    [SCShareableContent getShareableContentWithCompletionHandler:^(SCShareableContent *content, NSError *error) {
        [self install:content error:error];
    }];
}

- (void)install:(SCShareableContent *)content error:(NSError *)error {
    [self.lock lock];
    BOOL stopped = self.stopped;
    [self.lock unlock];
    if (stopped) return;
    if (error || content.displays.count == 0) {
        pinhole_report(self, error, "no display");
        return;
    }
    CGDirectDisplayID mainID = CGMainDisplayID();
    SCDisplay *display = content.displays[0];
    for (SCDisplay *candidate in content.displays) {
        if (candidate.displayID == mainID) display = candidate;
    }
    size_t width = CGDisplayPixelsWide(display.displayID);
    size_t height = CGDisplayPixelsHigh(display.displayID);
    if (width == 0 || height == 0) {
        width = (size_t)MAX(display.width, 0);
        height = (size_t)MAX(display.height, 0);
    }
    SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:display excludingWindows:@[]];
    SCStreamConfiguration *config = [SCStreamConfiguration new];
    config.width = width;
    config.height = height;
    config.pixelFormat = kCVPixelFormatType_32BGRA;
    config.showsCursor = YES;
    config.queueDepth = 3;
    // ponytail: 10 fps cap; raise it after measuring a phone on Wi-Fi.
    config.minimumFrameInterval = CMTimeMake(1, 10);
    NSError *addError = nil;
    SCStream *stream = [[SCStream alloc] initWithFilter:filter configuration:config delegate:self];
    if (![stream addStreamOutput:self type:SCStreamOutputTypeScreen sampleHandlerQueue:self.queue error:&addError]) {
        pinhole_report(self, addError, "could not capture the screen");
        return;
    }
    [self.lock lock];
    if (self.stopped) {
        [self.lock unlock];
        return;
    }
    self.stream = stream;
    [self.lock unlock];
    [stream startCaptureWithCompletionHandler:^(NSError *startError) {
        if (startError) pinhole_report(self, startError, "screen capture failed");
    }];
}

- (void)shutdownAndWait {
    [self.lock lock];
    self.stopped = YES;
    self.ctx = NULL;
    SCStream *stream = self.stream;
    self.stream = nil;
    [self.lock unlock];
    if (!stream) return;
    dispatch_semaphore_t done = dispatch_semaphore_create(0);
    [stream stopCaptureWithCompletionHandler:^(NSError *error) {
        (void)error;
        dispatch_semaphore_signal(done);
    }];
    dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 3 * NSEC_PER_SEC));
}

- (void)stream:(SCStream *)stream didOutputSampleBuffer:(CMSampleBufferRef)sample ofType:(SCStreamOutputType)type {
    (void)stream;
    if (type != SCStreamOutputTypeScreen) return;
    CFArrayRef attachments = CMSampleBufferGetSampleAttachmentsArray(sample, false);
    if (attachments && CFArrayGetCount(attachments) > 0) {
        CFDictionaryRef info = CFArrayGetValueAtIndex(attachments, 0);
        CFTypeRef status = CFDictionaryGetValue(info, (__bridge CFStringRef)SCStreamFrameInfoStatus);
        if (status && CFGetTypeID(status) == CFNumberGetTypeID()) {
            NSInteger value = 0;
            CFNumberGetValue((CFNumberRef)status, kCFNumberNSIntegerType, &value);
            if (value == SCFrameStatusIdle || value == SCFrameStatusSuspended || value == SCFrameStatusStopped) return;
        }
    }
    CVImageBufferRef buffer = CMSampleBufferGetImageBuffer(sample);
    if (!buffer || CVPixelBufferIsPlanar(buffer)) return;
    CVPixelBufferLockBaseAddress(buffer, kCVPixelBufferLock_ReadOnly);
    uint8_t *base = CVPixelBufferGetBaseAddress(buffer);
    size_t width = CVPixelBufferGetWidth(buffer);
    size_t height = CVPixelBufferGetHeight(buffer);
    size_t stride = CVPixelBufferGetBytesPerRow(buffer);
    [self.lock lock];
    void *ctx = self.stopped ? NULL : self.ctx;
    PinholeFrameFn onFrame = self.onFrame;
    [self.lock unlock];
    if (ctx && onFrame && base && width > 0 && height > 0 && width <= UINT32_MAX && height <= UINT32_MAX && stride <= UINT32_MAX) {
        onFrame(ctx, base, (uint32_t)width, (uint32_t)height, (uint32_t)stride);
    }
    CVPixelBufferUnlockBaseAddress(buffer, kCVPixelBufferLock_ReadOnly);
}

- (void)stream:(SCStream *)stream didStopWithError:(NSError *)error {
    (void)stream;
    pinhole_report(self, error, "screen capture stopped");
}
@end

void *pinhole_capture_start(void *ctx, PinholeFrameFn on_frame, PinholeErrorFn on_error) {
    PinholeCapture *cap = [PinholeCapture new];
    cap.ctx = ctx;
    cap.onFrame = on_frame;
    cap.onError = on_error;
    cap.lock = [NSLock new];
    cap.queue = dispatch_queue_create("pinhole.frames", DISPATCH_QUEUE_SERIAL);
    [cap begin];
    return (void *)CFBridgingRetain(cap);
}

void pinhole_capture_stop(void *raw) {
    if (!raw) return;
    PinholeCapture *cap = CFBridgingRelease(raw);
    [cap shutdownAndWait];
}

int pinhole_jpeg(const uint8_t *bgra, uint32_t width, uint32_t height, uint8_t **out_bytes, uint32_t *out_len) {
    if (!bgra || !out_bytes || !out_len || width == 0 || height == 0 || width > 256 || height > 256) return 1;
    *out_bytes = NULL;
    *out_len = 0;
    size_t bytes = (size_t)width * height * 4;
    CGColorSpaceRef space = CGColorSpaceCreateDeviceRGB();
    CGDataProviderRef provider = CGDataProviderCreateWithData(NULL, bgra, bytes, NULL);
    CGBitmapInfo info = kCGBitmapByteOrder32Little | kCGImageAlphaPremultipliedFirst;
    CGImageRef image = CGImageCreate(width, height, 8, 32, (size_t)width * 4, space, info, provider, NULL, false, kCGRenderingIntentDefault);
    CFMutableDataRef data = CFDataCreateMutable(kCFAllocatorDefault, 0);
    CGImageDestinationRef dest = image && data ? CGImageDestinationCreateWithData(data, CFSTR("public.jpeg"), 1, NULL) : NULL;
    if (dest) {
        NSDictionary *props = @{(__bridge NSString *)kCGImageDestinationLossyCompressionQuality: @0.92};
        CGImageDestinationAddImage(dest, image, (__bridge CFDictionaryRef)props);
        if (!CGImageDestinationFinalize(dest)) {
            CFRelease(dest);
            dest = NULL;
        }
    }
    int rc = 1;
    if (dest) {
        CFIndex len = CFDataGetLength(data);
        uint8_t *copy = len > 0 && len <= UINT32_MAX ? malloc((size_t)len) : NULL;
        if (copy) {
            memcpy(copy, CFDataGetBytePtr(data), (size_t)len);
            *out_bytes = copy;
            *out_len = (uint32_t)len;
            rc = 0;
        }
        CFRelease(dest);
    }
    if (data) CFRelease(data);
    if (image) CFRelease(image);
    if (provider) CFRelease(provider);
    if (space) CFRelease(space);
    return rc;
}

void pinhole_free(void *bytes) { free(bytes); }
