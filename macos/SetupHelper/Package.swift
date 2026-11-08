// swift-tools-version:6.2
// Looking Glass Setup: the macOS game-data helper (fork: macOS port).
// Built by tools/build_macos.py with `xcrun swift build -c release`; no dependencies.
import PackageDescription

let package = Package(
    name: "LookingGlassSetup",
    platforms: [.macOS(.v26)],
    targets: [
        .executableTarget(name: "LookingGlassSetup", path: "Sources/LookingGlassSetup")
    ]
)
