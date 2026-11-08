//! Platform layer for the macOS port.
//!
//! Every function here is a no-op (or the previous default) on other platforms, so
//! Windows and Linux behave exactly as before. macOS code lives in `macos.rs` and the
//! Objective-C shim in `macos/shim/`. Upstream files only call into this module.

#[cfg(target_os = "macos")]
mod macos;

use std::path::{Path, PathBuf};

/// Controller state in XInput's layout, which `input.rs` already understands.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PadState {
    pub buttons: u16,
    pub lt: u8,
    pub rt: u8,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
}

/// Choose the folder that holds `private/` (saves, settings, imported data), then
/// make sure `private/` exists.
///
/// macOS only. `LOOKING_GLASS_HOME` wins if set. Otherwise, when the executable runs
/// from inside an app bundle, use `~/Library/Application Support/Looking Glass`,
/// because Finder starts apps with `/` as the working directory. Terminal and
/// `cargo run` launches keep the current directory, as on Windows.
pub fn init_working_dir() -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use anyhow::Context;
        let explicit = std::env::var_os("LOOKING_GLASS_HOME").map(PathBuf::from);
        let bundled = std::env::current_exe()
            .ok()
            .zip(std::env::var_os("HOME").map(PathBuf::from))
            .and_then(|(exe, home)| bundle_home(&exe, &home));
        if let Some(dir) = explicit.or(bundled) {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("create game folder {}", dir.display()))?;
            std::env::set_current_dir(&dir)
                .with_context(|| format!("enter game folder {}", dir.display()))?;
        }
        std::fs::create_dir_all("private").context("create private folder")?;
    }
    Ok(())
}

/// The per-user data folder when `exe` is inside a `.app` bundle, otherwise `None`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn bundle_home(exe: &Path, home: &Path) -> Option<PathBuf> {
    let parts: Vec<_> = exe.components().map(|c| c.as_os_str()).collect();
    let in_bundle = parts.windows(3).any(|w| {
        w[0].to_str().is_some_and(|s| s.ends_with(".app"))
            && w[1] == "Contents"
            && w[2] == "MacOS"
    });
    in_bundle.then(|| home.join("Library/Application Support/Looking Glass"))
}

/// State of controller `index` (0-based), or `None` when not connected.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn pad_state(index: u32) -> Option<PadState> {
    #[cfg(target_os = "macos")]
    {
        macos::pad_state(index)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = index;
        None
    }
}

/// Set the windowed size from the menu, in framebuffer pixels.
///
/// macOS: miniquad 0.4.8 sizes the whole frame (title bar included) in points, so
/// "1920 x 1080" became a 3840-pixel-wide window off the edge of a laptop screen. The
/// shim converts pixels to points, sizes the content, and fits it on screen.
/// Elsewhere this is exactly the previous `miniquad::window::set_window_size` call.
pub fn set_window_size(width: u32, height: u32) {
    #[cfg(target_os = "macos")]
    if macos::set_window_pixels(width, height) {
        return;
    }
    macroquad::miniquad::window::set_window_size(width, height);
}

/// Hold each frame to a steady deadline at the display's refresh rate (macOS only).
///
/// miniquad 0.4.8 on macOS draws from both its free-running event loop and a 16 ms
/// NSTimer, and OpenGL vsync is unreliable on ProMotion screens, so frame spacing
/// varied between ~9.7 and ~15.6 ms (5th–95th percentile; measured). Call once at the
/// start of each frame. `LOOKING_GLASS_FPS` overrides the rate; `0` disables pacing.
pub fn pace_frame() {
    #[cfg(target_os = "macos")]
    PACER.with(|p| p.borrow_mut().wait());
}

#[cfg(target_os = "macos")]
thread_local! {
    static PACER: std::cell::RefCell<Pacer> = std::cell::RefCell::new(Pacer::default());
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Default)]
struct Pacer {
    deadline: Option<std::time::Instant>,
    hz: u32,
    checked: Option<std::time::Instant>,
}

#[cfg(target_os = "macos")]
impl Pacer {
    fn wait(&mut self) {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        // Re-read the rate now and then: the window can move between screens.
        if self.checked.is_none_or(|t| now - t > Duration::from_secs(2)) {
            self.checked = Some(now);
            self.hz = match std::env::var("LOOKING_GLASS_FPS").ok().and_then(|v| v.parse().ok()) {
                Some(hz) => hz,
                None => macos::display_refresh_hz().unwrap_or(60),
            };
        }
        if self.hz == 0 {
            self.deadline = None;
            return;
        }
        let period = Duration::from_secs_f64(1. / self.hz.clamp(24, 360) as f64);
        let (wait, next) = next_deadline(self.deadline, now, period);
        if let Some(wait) = wait {
            // Sleep most of the way, then spin the last millisecond for precision.
            if let Some(coarse) = wait.checked_sub(Duration::from_millis(1)) {
                std::thread::sleep(coarse);
            }
            let target = now + wait;
            while Instant::now() < target {
                std::hint::spin_loop();
            }
        }
        self.deadline = Some(next);
    }
}

/// How long to wait before starting this frame, and the following frame's deadline.
/// A frame that is more than one period late re-anchors instead of bursting to catch up.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn next_deadline(
    deadline: Option<std::time::Instant>,
    now: std::time::Instant,
    period: std::time::Duration,
) -> (Option<std::time::Duration>, std::time::Instant) {
    match deadline {
        Some(d) if now < d => (Some(d - now), d + period),
        Some(d) if now - d < period => (None, d + period),
        _ => (None, now + period),
    }
}

/// Whether the game window is frontmost and has keyboard focus.
/// (Windows keeps its own Win32 check in `look.rs`, so it never calls this.)
#[cfg_attr(windows, allow(dead_code))]
pub fn window_focused() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::window_focused()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_executable_uses_application_support() {
        let home = Path::new("/Users/alice");
        assert_eq!(
            bundle_home(
                Path::new("/Applications/Looking Glass.app/Contents/MacOS/looking-glass"),
                home
            ),
            Some(PathBuf::from(
                "/Users/alice/Library/Application Support/Looking Glass"
            ))
        );
    }

    #[test]
    fn pacing_waits_for_the_deadline_then_advances_one_period() {
        use std::time::{Duration, Instant};
        let period = Duration::from_millis(8);
        let t0 = Instant::now();
        // First frame anchors.
        assert_eq!(next_deadline(None, t0, period), (None, t0 + period));
        // Early: wait the remainder, next deadline one period later.
        let d = t0 + period;
        assert_eq!(
            next_deadline(Some(d), t0 + Duration::from_millis(5), period),
            (Some(Duration::from_millis(3)), d + period)
        );
        // Slightly late: no wait, keep the cadence.
        assert_eq!(
            next_deadline(Some(d), d + Duration::from_millis(2), period),
            (None, d + period)
        );
        // Very late (a hitch): re-anchor instead of bursting frames.
        let late = d + Duration::from_millis(50);
        assert_eq!(next_deadline(Some(d), late, period), (None, late + period));
    }

    #[test]
    fn terminal_builds_keep_their_directory() {
        let home = Path::new("/Users/alice");
        for exe in [
            "/src/LookingGlass/target/release/looking-glass",
            "/src/Not.app.d/Contents/MacOS/looking-glass",
            "/src/Game.app/Contents/Resources/looking-glass",
            "/src/Game.app/MacOS/looking-glass",
        ] {
            assert_eq!(bundle_home(Path::new(exe), home), None, "{exe}");
        }
    }
}
