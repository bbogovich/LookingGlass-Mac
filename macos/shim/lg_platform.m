// macOS platform shim for Looking Glass.
//
// Compiled by build.rs only when targeting macOS (xcrun clang + ar, no extra crates)
// and called from src/platform/macos.rs. It exposes a tiny C ABI so the engine's
// existing XInput-shaped controller code (src/input.rs) works unchanged on macOS.
//
// Controllers come from GameController.framework: Xbox, PlayStation (DualShock /
// DualSense), Switch Pro and MFi pads that provide an extended gamepad profile.

#import <AppKit/AppKit.h>
#import <GameController/GameController.h>
#import <objc/runtime.h>
#include <stdint.h>

// Must match `LgPad` in src/platform/macos.rs (and the XInput XINPUT_GAMEPAD layout).
typedef struct {
    uint16_t buttons;
    uint8_t lt;
    uint8_t rt;
    int16_t lx;
    int16_t ly;
    int16_t rx;
    int16_t ry;
} LgPad;

// XInput button bits, as used by src/input.rs PAD_BUTTONS.
enum {
    LG_DPAD_UP = 0x0001,
    LG_DPAD_DOWN = 0x0002,
    LG_DPAD_LEFT = 0x0004,
    LG_DPAD_RIGHT = 0x0008,
    LG_START = 0x0010,
    LG_BACK = 0x0020,
    LG_LEFT_THUMB = 0x0040,
    LG_RIGHT_THUMB = 0x0080,
    LG_LEFT_SHOULDER = 0x0100,
    LG_RIGHT_SHOULDER = 0x0200,
    LG_A = 0x1000,
    LG_B = 0x2000,
    LG_X = 0x4000,
    LG_Y = 0x8000,
};

// A GameController stick value (-1...1) as an XInput thumb value.
static int16_t lg_axis(float value) {
    if (value != value) return 0;  // NaN
    if (value > 1.0f) value = 1.0f;
    if (value < -1.0f) value = -1.0f;
    return (int16_t)(value * 32767.0f);
}

// A GameController trigger value (0...1) as an XInput trigger value.
static uint8_t lg_trigger(float value) {
    if (value != value || value <= 0.0f) return 0;
    if (value >= 1.0f) return 255;
    return (uint8_t)(value * 255.0f + 0.5f);
}

// `bit` when the button is held, so buttons can be OR-ed into the XInput mask.
static uint16_t lg_bit(GCControllerButtonInput *button, uint16_t bit) {
    return (button != nil && button.isPressed) ? bit : 0;
}

// The `index`-th connected controller that has an extended gamepad profile, or nil.
static GCExtendedGamepad *lg_gamepad(uint32_t index) {
    uint32_t seen = 0;
    for (GCController *controller in [GCController controllers]) {
        GCExtendedGamepad *pad = controller.extendedGamepad;
        if (pad == nil) continue;
        if (seen == index) return pad;
        seen++;
    }
    return nil;
}

// Returns 1 and fills `out` when controller `index` is connected, otherwise 0.
int32_t lg_pad_state(uint32_t index, LgPad *out) {
    if (out == NULL) return 0;
    @autoreleasepool {
        GCExtendedGamepad *pad = lg_gamepad(index);
        if (pad == nil) return 0;
        LgPad state = {0};
        state.buttons = lg_bit(pad.dpad.up, LG_DPAD_UP) | lg_bit(pad.dpad.down, LG_DPAD_DOWN) |
                        lg_bit(pad.dpad.left, LG_DPAD_LEFT) | lg_bit(pad.dpad.right, LG_DPAD_RIGHT) |
                        lg_bit(pad.buttonMenu, LG_START) | lg_bit(pad.buttonOptions, LG_BACK) |
                        lg_bit(pad.leftThumbstickButton, LG_LEFT_THUMB) |
                        lg_bit(pad.rightThumbstickButton, LG_RIGHT_THUMB) |
                        lg_bit(pad.leftShoulder, LG_LEFT_SHOULDER) |
                        lg_bit(pad.rightShoulder, LG_RIGHT_SHOULDER) | lg_bit(pad.buttonA, LG_A) |
                        lg_bit(pad.buttonB, LG_B) | lg_bit(pad.buttonX, LG_X) |
                        lg_bit(pad.buttonY, LG_Y);
        state.lt = lg_trigger(pad.leftTrigger.value);
        state.rt = lg_trigger(pad.rightTrigger.value);
        // GameController and XInput both report stick Y as positive when pushed up.
        state.lx = lg_axis(pad.leftThumbstick.xAxis.value);
        state.ly = lg_axis(pad.leftThumbstick.yAxis.value);
        state.rx = lg_axis(pad.rightThumbstick.xAxis.value);
        state.ry = lg_axis(pad.rightThumbstick.yAxis.value);
        *out = state;
        return 1;
    }
}

// Highest refresh rate of the screen showing the game window (ProMotion reports 120),
// or of the main screen before the window exists. 0 when unknown.
int32_t lg_display_refresh_hz(void) {
    @autoreleasepool {
        NSApplication *app = NSApp;
        NSScreen *screen = app.keyWindow.screen ?: app.mainWindow.screen ?: NSScreen.mainScreen;
        return screen != nil ? (int32_t)screen.maximumFramesPerSecond : 0;
    }
}

// Resize the game window so its *content* is `width` x `height` framebuffer pixels
// (the game's menu sizes are pixels; AppKit sizes are points), shrunk to fit the
// screen's visible area with the aspect ratio kept, and moved fully on screen.
// Returns 1 when a window was resized. Does nothing in fullscreen.
int32_t lg_set_window_pixels(uint32_t width, uint32_t height) {
    @autoreleasepool {
        NSApplication *app = NSApp;
        NSWindow *window = app.mainWindow ?: app.keyWindow ?: app.windows.firstObject;
        if (window == nil || width == 0 || height == 0) return 0;
        if ((window.styleMask & NSWindowStyleMaskFullScreen) != 0) return 0;
        CGFloat scale = window.backingScaleFactor > 0 ? window.backingScaleFactor : 1;
        NSSize content = NSMakeSize(width / scale, height / scale);
        NSScreen *screen = window.screen ?: NSScreen.mainScreen;
        if (screen != nil) {
            NSRect visible = screen.visibleFrame;
            // Room for content = visible area minus the window's frame decorations.
            NSRect probe = [window frameRectForContentRect:NSMakeRect(0, 0, 100, 100)];
            CGFloat maxW = visible.size.width - (probe.size.width - 100);
            CGFloat maxH = visible.size.height - (probe.size.height - 100);
            CGFloat fit = MIN(1.0, MIN(maxW / content.width, maxH / content.height));
            content = NSMakeSize(floor(content.width * fit), floor(content.height * fit));
        }
        NSRect frame = window.frame;
        NSRect target = [window frameRectForContentRect:NSMakeRect(0, 0, content.width, content.height)];
        // Keep the top-left corner where it was, then pull the frame back on screen.
        target.origin = NSMakePoint(frame.origin.x, NSMaxY(frame) - target.size.height);
        if (screen != nil) {
            NSRect visible = screen.visibleFrame;
            target.origin.x = MIN(MAX(target.origin.x, NSMinX(visible)), NSMaxX(visible) - target.size.width);
            target.origin.y = MIN(MAX(target.origin.y, NSMinY(visible)), NSMaxY(visible) - target.size.height);
        }
        [window setFrame:target display:YES animate:NO];
        return 1;
    }
}

// True while the Option key is held (checked at launch to open Setup).
int32_t lg_option_held(void) {
    return (NSEvent.modifierFlags & NSEventModifierFlagOption) != 0 ? 1 : 0;
}

// The window's latest fullscreen state, until read: -1 none, 0 windowed, 1 fullscreen.
static int32_t lg_fullscreen_change = -1;

// Read and clear the latest fullscreen change (see lg_fullscreen_change).
int32_t lg_take_fullscreen_change(void) {
    int32_t change = lg_fullscreen_change;
    lg_fullscreen_change = -1;
    return change;
}

// Cmd+Return toggles fullscreen in every game loop. The event is consumed here, so the
// game never sees it as Enter (which would skip a scene or press a menu button).
// Cmd+Q and the Dock's Quit call -terminate:, which miniquad lets end the process at once,
// skipping the game's exit autosave. Close the window instead: miniquad turns that into the
// quit request every game loop already handles. With no window left, quit as normal.
static NSApplicationTerminateReply lg_should_terminate(id self, SEL _cmd, NSApplication *app) {
    NSWindow *window = app.windows.firstObject;
    if (window == nil || !window.visible) return NSTerminateNow;
    [window performClose:nil];
    return NSTerminateCancel;
}

// Install the Quit, Cmd+Return and fullscreen hooks once, after AppKit has started.
static void lg_install_window_hooks(void) {
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        Class delegate = [NSApp.delegate class];
        if (delegate != Nil) {
            class_addMethod(delegate, @selector(applicationShouldTerminate:),
                            (IMP)lg_should_terminate, "Q@:@");
        }
        enum { LG_KEY_RETURN = 36, LG_KEY_KEYPAD_ENTER = 76 };
        NSEventModifierFlags relevant = NSEventModifierFlagCommand | NSEventModifierFlagShift |
                                        NSEventModifierFlagOption | NSEventModifierFlagControl;
        [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskKeyDown
                                              handler:^NSEvent *(NSEvent *event) {
            BOOL enter = event.keyCode == LG_KEY_RETURN || event.keyCode == LG_KEY_KEYPAD_ENTER;
            if (!enter || (event.modifierFlags & relevant) != NSEventModifierFlagCommand) return event;
            if (!event.isARepeat) [NSApp.keyWindow toggleFullScreen:nil];
            return nil;
        }];
        NSNotificationCenter *center = NSNotificationCenter.defaultCenter;
        [center addObserverForName:NSWindowDidEnterFullScreenNotification object:nil queue:nil
                        usingBlock:^(NSNotification *note) { lg_fullscreen_change = 1; }];
        [center addObserverForName:NSWindowDidExitFullScreenNotification object:nil queue:nil
                        usingBlock:^(NSNotification *note) { lg_fullscreen_change = 0; }];
    });
}

// True when this application is frontmost and owns the key window. Every game loop polls
// this once AppKit is running, which makes it the place to install the window hooks.
int32_t lg_window_focused(void) {
    @autoreleasepool {
        NSApplication *app = NSApp;
        if (app != nil) lg_install_window_hooks();
        return (app != nil && app.isActive && app.keyWindow != nil) ? 1 : 0;
    }
}
