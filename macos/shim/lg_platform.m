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

static int16_t lg_axis(float value) {
    if (value != value) return 0;  // NaN
    if (value > 1.0f) value = 1.0f;
    if (value < -1.0f) value = -1.0f;
    return (int16_t)(value * 32767.0f);
}

static uint8_t lg_trigger(float value) {
    if (value != value || value <= 0.0f) return 0;
    if (value >= 1.0f) return 255;
    return (uint8_t)(value * 255.0f + 0.5f);
}

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

// True when this application is frontmost and owns the key window.
int32_t lg_window_focused(void) {
    @autoreleasepool {
        NSApplication *app = NSApp;
        return (app != nil && app.isActive && app.keyWindow != nil) ? 1 : 0;
    }
}
