fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // A level change loads the destination while the viewer and outgoing
        // scene are still live. Windows' 1 MiB default exhausts the main thread
        // stack in that path; reserve 8 MiB, with pages committed on demand.
        // RUST_MIN_STACK only affects spawned threads, not the window thread.
        let arg = if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            "/STACK:8388608"
        } else {
            "-Wl,--stack,8388608"
        };
        println!("cargo:rustc-link-arg={arg}");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        macos_shim(); // fork: see macos/shim/lg_platform.m
    }
}

/// Compile the macOS Objective-C shim with the system toolchain (no extra crates).
fn macos_shim() {
    use std::process::Command;
    let source = "macos/shim/lg_platform.m";
    println!("cargo:rerun-if-changed={source}");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok(other) => panic!("unsupported macOS architecture: {other}"),
        Err(e) => panic!("CARGO_CFG_TARGET_ARCH: {e}"),
    };
    // Match rustc's own deployment target so the linker sees one consistent minimum.
    let min = std::env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| "11.0".into());
    let object = out.join("lg_platform.o");
    let run = |command: &mut Command| {
        let status = command
            .status()
            .unwrap_or_else(|e| panic!("run {command:?}: {e}"));
        assert!(status.success(), "{command:?} failed with {status}");
    };
    run(Command::new("xcrun")
        .args(["clang", "-c", "-fobjc-arc", "-O2", "-Wall"])
        .args(["-target", &format!("{arch}-apple-macos{min}")])
        .arg(source)
        .arg("-o")
        .arg(&object));
    run(Command::new("xcrun")
        .args(["ar", "crs"])
        .arg(out.join("liblg_platform.a"))
        .arg(&object));
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=lg_platform");
    for framework in ["GameController", "AppKit", "Foundation"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
