fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    co_op_build_identity();
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
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg=/EXPORT:NvOptimusEnablement,DATA");
        }
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

/// Version strings alone cannot distinguish two development builds on `coop`.
/// Hash the actual sources and locked dependencies, without requiring Git or
/// embedding private paths. CRLF/LF checkouts produce the same identity.
fn co_op_build_identity() {
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    fn collect(dir: &Path, paths: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read build source directory") {
            let path = entry.expect("read build source entry").path();
            if path.is_dir() {
                collect(&path, paths);
            } else {
                paths.push(path);
            }
        }
    }
    let mut paths = vec![
        PathBuf::from("Cargo.toml"),
        PathBuf::from("Cargo.lock"),
        PathBuf::from("build.rs"),
    ];
    collect(Path::new("src"), &mut paths);
    paths.sort();
    let mut hash = Sha256::new();
    hash.update(b"Looking Glass co-op source identity 1\0");
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path.to_string_lossy().replace('\\', "/");
        let raw = fs::read(&path).expect("read build source");
        let bytes = match std::str::from_utf8(&raw) {
            Ok(text) => text.replace("\r\n", "\n").into_bytes(),
            Err(_) => raw,
        };
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    // Observe added/removed source files as well as edits to known files.
    println!("cargo:rerun-if-changed=src");
    hash.update(std::env::var("TARGET").expect("build target").as_bytes());
    println!("cargo:rustc-env=LOOKING_GLASS_BUILD={:x}", hash.finalize());
}
