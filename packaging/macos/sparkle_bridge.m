// Minimal Sparkle bridge: load Sparkle.framework from the app bundle at runtime.
// Safe no-op when framework or symbols are missing (dev builds / CI).

#import <Foundation/Foundation.h>
#import <objc/message.h>
#import <objc/runtime.h>

static Class keel_sparkle_controller_class(void) {
  NSBundle *main = [NSBundle mainBundle];
  NSString *fwPath =
      [[main privateFrameworksPath] stringByAppendingPathComponent:@"Sparkle.framework"];
  NSBundle *sparkle = [NSBundle bundleWithPath:fwPath];
  if (!sparkle) {
    // Also try Frameworks next to the executable (non-.app layouts)
    NSString *exe = [[NSProcessInfo processInfo] arguments].firstObject;
    NSString *dir = [exe stringByDeletingLastPathComponent];
    fwPath = [[dir stringByAppendingPathComponent:@"../Frameworks/Sparkle.framework"]
        stringByStandardizingPath];
    sparkle = [NSBundle bundleWithPath:fwPath];
  }
  if (!sparkle || ![sparkle load]) {
    return Nil;
  }
  return NSClassFromString(@"SPUStandardUpdaterController");
}

void keel_sparkle_init(void) {
  @autoreleasepool {
    Class cls = keel_sparkle_controller_class();
    if (!cls) {
      return;
    }
    // SPUStandardUpdaterController *c = [[SPUStandardUpdaterController alloc]
    //   initWithStartingUpdater:YES updaterDelegate:nil userDriverDelegate:nil];
    id controller = ((id(*)(id, SEL, BOOL, id, id))objc_msgSend)(
        [cls alloc],
        sel_registerName("initWithStartingUpdater:updaterDelegate:userDriverDelegate:"),
        YES,
        nil,
        nil);
    // Retain for process lifetime
    static id retained = nil;
    retained = controller;
    (void)retained;
  }
}

void keel_sparkle_check_for_updates(void) {
  @autoreleasepool {
    Class cls = keel_sparkle_controller_class();
    if (!cls) {
      NSLog(@"Keel: Sparkle.framework not found — skipping update check");
      return;
    }
    static id controller = nil;
    if (!controller) {
      controller = ((id(*)(id, SEL, BOOL, id, id))objc_msgSend)(
          [cls alloc],
          sel_registerName("initWithStartingUpdater:updaterDelegate:userDriverDelegate:"),
          YES,
          nil,
          nil);
    }
    id updater = ((id(*)(id, SEL))objc_msgSend)(controller, sel_registerName("updater"));
    if (updater) {
      ((void (*)(id, SEL, id))objc_msgSend)(
          updater, sel_registerName("checkForUpdates:"), nil);
    }
  }
}
