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
