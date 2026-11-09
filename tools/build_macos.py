#!/usr/bin/env python3
"""Build the macOS app: python3 tools/build_macos.py [--install] [--data PATH] [--no-data]

Produces build/macos/Looking Glass.app. If an Alice1 folder is present (or --data names
a folder or archive), its game files are built into the app, so it runs on its own.
That app then contains game data: it is for your own use, so do not share it.
Saves and settings always live in ~/Library/Application Support/Looking Glass.
Needs the Xcode Command Line Tools and Rust (rustup). Standard library only.
"""
import argparse
import json
import os
import platform
import plistlib
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "build" / "macos"
APP = OUT / "Looking Glass.app"
TARGET = "aarch64-apple-darwin"
MIN_MACOS = "26.0"
BUNDLE_ID = "io.github.bbogovich.lookingglass"
PORTRAIT = "ui/main/alicec.tga"


def fail(message):
    """Stop with a message; the build script never shows a traceback for expected problems."""
    sys.exit(f"build_macos: {message}")


def run(*command, **options):
    """Run a command, stopping the build if it fails."""
    subprocess.run([str(part) for part in command], check=True, **options)


def version():
    """The game version from Cargo.toml, used as the app version."""
    for line in (ROOT / "Cargo.toml").read_text().splitlines():
        if line.startswith("version"):
            return line.split('"')[1]
    fail("no version in Cargo.toml")


def find_cargo():
    """Cargo and a PATH that includes rustup's usual folders, which a GUI shell may lack."""
    extra = [Path.home() / ".cargo/bin", Path("/opt/homebrew/opt/rustup/bin")]
    path = os.pathsep.join([os.environ.get("PATH", "")] + [str(p) for p in extra])
    cargo = shutil.which("cargo", path=path)
    if not cargo:
        fail("Rust was not found. Install it from https://rustup.rs, then run this again.")
    return cargo, path


def check_system():
    """Refuse to build where the result could not run: Intel Macs, old macOS, no Command Line Tools."""
    if sys.platform != "darwin" or platform.machine() != "arm64":
        fail("this builds on Apple silicon Macs only.")
    if int(platform.mac_ver()[0].split(".")[0]) < int(MIN_MACOS.split(".")[0]):
        fail(f"macOS {MIN_MACOS} or newer is required.")
    if subprocess.run(["xcrun", "--find", "swift"], capture_output=True).returncode != 0:
        fail("the Xcode Command Line Tools are missing. Run: xcode-select --install")


def info_plist(executable, name, identifier, icon):
    """The Info.plist for the game or its Setup helper."""
    plist = {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleExecutable": executable,
        "CFBundleIdentifier": identifier,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": name,
        "CFBundleDisplayName": name,
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version(),
        "CFBundleVersion": version(),
        "LSApplicationCategoryType": "public.app-category.games",
        "LSArchitecturePriority": ["arm64"],
        "LSMinimumSystemVersion": MIN_MACOS,
        "NSHighResolutionCapable": True,
        "GCSupportsControllerUserInteraction": True,
    }
    if icon:
        plist["CFBundleIconFile"] = "AppIcon"
    return plist


def write_app(app, executable, source, name, identifier, icon):
    """Assemble one app bundle: executable, Info.plist and icon."""
    contents = app / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Resources").mkdir(exist_ok=True)
    shutil.copy2(source, contents / "MacOS" / executable)
    with open(contents / "Info.plist", "wb") as file:
        plistlib.dump(info_plist(executable, name, identifier, icon is not None), file)
    if icon:
        shutil.copy2(icon, contents / "Resources" / "AppIcon.icns")


def import_data(engine, source, resources, language):
    """Check and import with the engine's own rules, then keep the result in the app."""
    print(f"Importing game data from {source}")
    with tempfile.TemporaryDirectory(dir=OUT) as home:
        arguments = [engine, "--setup-data", source] + (["--language", language] if language else [])
        process = subprocess.run([str(a) for a in arguments], capture_output=True, text=True,
                                 env={**os.environ, "LOOKING_GLASS_HOME": home})
        lines = [json.loads(l) for l in process.stdout.splitlines() if l.startswith("{")]
        final = lines[-1] if lines else {}
        if not final.get("done"):
            fail(f"the game data was not accepted: {final.get('error') or process.stderr.strip()}")
        report = final["report"]
        data = resources / "game-data"
        data.mkdir()
        shutil.move(Path(report["base"]), data / "base")
        shutil.move(Path(home) / "private/game-data/languages", data / "languages")
    for warning in report["warnings"]:
        print(f"  note: {warning}")
    print(f"  {report['maps']} maps; languages: {', '.join(report['languages'])}; active: {report['language']}")
    return data / "base"


def make_icon(base):
    """Build AppIcon.icns from the portrait in the player's packs, or None without it."""
    work = OUT / "icon"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir()
    for pack in sorted(base.glob("*.pk3")):
        with zipfile.ZipFile(pack) as archive:
            name = next((n for n in archive.namelist() if n.replace("\\", "/").lower() == PORTRAIT), None)
            if name:
                (work / "portrait.tga").write_bytes(archive.read(name))
                break
    else:
        print("  note: no portrait found in the game data; the app gets the generic icon.")
        return None
    tool = work / "make_icon"
    run("xcrun", "swiftc", "-O", ROOT / "macos/icon/make_icon.swift", "-o", tool)
    run(tool, work / "portrait.tga", work / "AppIcon.iconset")
    run("iconutil", "-c", "icns", work / "AppIcon.iconset", "-o", work / "AppIcon.icns")
    return work / "AppIcon.icns"


def sign(app, helper):
    """Ad-hoc sign the helper, then the app, so the seal covers everything inside."""
    # Finder attributes on copied files would make codesign refuse the bundle.
    run("xattr", "-cr", app)
    for path in (helper, app):
        run("codesign", "--force", "--sign", "-", path)
    run("codesign", "--verify", "--deep", "--strict", app)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--data", type=Path, help="game folder or archive to build in (default: ./Alice1 if present)")
    parser.add_argument("--no-data", action="store_true", help="build without game data; Setup asks on first launch")
    parser.add_argument("--language", help="language code for the built-in data (default: the data's own, else English)")
    parser.add_argument("--install", action="store_true", help="also copy the app to ~/Applications")
    args = parser.parse_args()

    check_system()
    cargo, path = find_cargo()
    env = {**os.environ, "PATH": path, "MACOSX_DEPLOYMENT_TARGET": MIN_MACOS}
    print("Building the game...")
    run(cargo, "build", "--release", "--locked", "--target", TARGET, cwd=ROOT, env=env)
    print("Building the Setup helper...")
    run("xcrun", "swift", "build", "-c", "release", "--package-path", ROOT / "macos/SetupHelper", env=env)

    OUT.mkdir(parents=True, exist_ok=True)
    shutil.rmtree(APP, ignore_errors=True)
    engine = ROOT / "target" / TARGET / "release" / "looking-glass"
    helper_exe = ROOT / "macos/SetupHelper/.build/release/LookingGlassSetup"
    contents = APP / "Contents"
    (contents / "Helpers").mkdir(parents=True)
    resources = contents / "Resources"
    resources.mkdir()

    source = args.data or (ROOT / "Alice1" if (ROOT / "Alice1").is_dir() else None)
    base = None
    if source and not args.no_data:
        if not source.exists():
            fail(f"{source} does not exist.")
        base = import_data(engine, source.resolve(), resources, args.language)
    else:
        print("No game data built in; Setup will ask for it on first launch.")
    icon = make_icon(base) if base else None

    helper = contents / "Helpers" / "Looking Glass Setup.app"
    write_app(helper, "LookingGlassSetup", helper_exe, "Looking Glass Setup", BUNDLE_ID + ".setup", icon)
    write_app(APP, "looking-glass", engine, "Looking Glass", BUNDLE_ID, icon)
    for notice in ("LEGAL.md", "THIRD_PARTY_NOTICES.txt"):
        if (ROOT / notice).is_file():
            shutil.copy2(ROOT / notice, resources / notice)

    print("Signing...")
    sign(APP, helper)
    print(f"Built {APP}")
    if args.install:
        target = Path.home() / "Applications" / APP.name
        target.parent.mkdir(exist_ok=True)
        shutil.rmtree(target, ignore_errors=True)
        run("ditto", APP, target)
        print(f"Installed {target}")


if __name__ == "__main__":
    main()
