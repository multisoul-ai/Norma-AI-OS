// CGVirtualDisplay is private CoreGraphics API (also used by DeskPad / BetterDisplay).
// Debug E2E only: the isolated Norma window opens on this display, so the run never
// covers the user's screens.
#import <Foundation/Foundation.h>
#import <CoreGraphics/CoreGraphics.h>
#import <unistd.h>
#import "VirtualDisplay.h"

@interface CGVirtualDisplayDescriptor : NSObject
@property (retain) dispatch_queue_t queue;
@property (copy) NSString *name;
@property unsigned int maxPixelsHigh;
@property unsigned int maxPixelsWide;
@property CGSize sizeInMillimeters;
@property unsigned int productID;
@property unsigned int vendorID;
@property unsigned int serialNum;
@property (copy) void (^terminationHandler)(id, id);
@end

@interface CGVirtualDisplayMode : NSObject
- (instancetype)initWithWidth:(unsigned int)width height:(unsigned int)height refreshRate:(double)rate;
@end

@interface CGVirtualDisplaySettings : NSObject
@property (retain) NSArray *modes;
@property unsigned int hiDPI;
@end

@interface CGVirtualDisplay : NSObject
- (instancetype)initWithDescriptor:(CGVirtualDisplayDescriptor *)descriptor;
- (BOOL)applySettings:(CGVirtualDisplaySettings *)settings;
@property (readonly) CGDirectDisplayID displayID;
@end

static CGVirtualDisplay *sharedDisplay;

uint32_t NormaE2ECreateVirtualDisplay(uint32_t width, uint32_t height) {
    if (sharedDisplay) return sharedDisplay.displayID;
    Class displayClass = NSClassFromString(@"CGVirtualDisplay");
    if (!displayClass) return 0;
    CGVirtualDisplayDescriptor *descriptor = [NSClassFromString(@"CGVirtualDisplayDescriptor") new];
    descriptor.queue = dispatch_queue_create("norma.e2e.virtual-display", DISPATCH_QUEUE_SERIAL);
    descriptor.name = @"Norma E2E";
    descriptor.maxPixelsWide = width * 2;
    descriptor.maxPixelsHigh = height * 2;
    descriptor.sizeInMillimeters = CGSizeMake(width * 25.4 / 110, height * 25.4 / 110);
    descriptor.vendorID = 0x4E4F;
    descriptor.productID = 0xE2E0;
    descriptor.serialNum = (unsigned int)getpid();
    descriptor.terminationHandler = ^(id display, id error) {};

    CGVirtualDisplay *display = [[displayClass alloc] initWithDescriptor:descriptor];
    if (!display) return 0;
    CGVirtualDisplaySettings *settings = [NSClassFromString(@"CGVirtualDisplaySettings") new];
    settings.hiDPI = 1;
    settings.modes = @[[[NSClassFromString(@"CGVirtualDisplayMode") alloc] initWithWidth:width height:height refreshRate:60]];
    if (![display applySettings:settings]) return 0;

    // Only the corner touches the main display, so the user's pointer rarely wanders in.
    CGRect main = CGDisplayBounds(CGMainDisplayID());
    CGDisplayConfigRef config;
    if (CGBeginDisplayConfiguration(&config) == kCGErrorSuccess) {
        CGConfigureDisplayOrigin(config, display.displayID, (int32_t)CGRectGetMaxX(main), (int32_t)CGRectGetMaxY(main));
        CGCompleteDisplayConfiguration(config, kCGConfigureForSession);
    }
    sharedDisplay = display;
    return display.displayID;
}
