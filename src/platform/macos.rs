//! Rust side of the Objective-C shim in `macos/shim/lg_platform.m`.
use super::PadState;

/// Must match `LgPad` in the shim.
#[repr(C)]
#[derive(Default)]
struct LgPad {
    buttons: u16,
    lt: u8,
    rt: u8,
    lx: i16,
    ly: i16,
    rx: i16,
    ry: i16,
}

extern "C" {
    fn lg_pad_state(index: u32, out: *mut LgPad) -> i32;
    fn lg_window_focused() -> i32;
    fn lg_display_refresh_hz() -> i32;
    fn lg_set_window_pixels(width: u32, height: u32) -> i32;
}

/// Size the window content in framebuffer pixels, fitted to the screen. False when
/// no window was available (the caller then falls back to miniquad).
pub fn set_window_pixels(width: u32, height: u32) -> bool {
    // Plain integers; the shim reads and resizes NSApp's window on the main thread.
    unsafe { lg_set_window_pixels(width, height) == 1 }
}

/// Refresh rate of the game's screen in Hz, if AppKit reports one.
pub fn display_refresh_hz() -> Option<u32> {
    // No arguments; reads NSScreen state on the main thread that owns the window.
    let hz = unsafe { lg_display_refresh_hz() };
    (hz > 0).then_some(hz as u32)
}

pub fn pad_state(index: u32) -> Option<PadState> {
    let mut raw = LgPad::default();
    // `raw` is a valid, writable LgPad with the shim's exact C layout; the shim checks
    // the index and writes only on success. Called from the main (AppKit) thread.
    (unsafe { lg_pad_state(index, &mut raw) } == 1).then_some(PadState {
        buttons: raw.buttons,
        lt: raw.lt,
        rt: raw.rt,
        lx: raw.lx,
        ly: raw.ly,
        rx: raw.rx,
        ry: raw.ry,
    })
}

pub fn window_focused() -> bool {
    // No arguments; reads NSApp state on the main thread that owns the window.
    unsafe { lg_window_focused() == 1 }
}
