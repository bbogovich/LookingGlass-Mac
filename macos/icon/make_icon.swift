// Builds an .iconset from the player's own copy of the game's Alice portrait.
// The art is read from their packs at build time; none of it is stored in this repo.
// Usage: make_icon <source image> <output .iconset folder>
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let arguments = CommandLine.arguments
guard arguments.count == 3,
      let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: arguments[1]) as CFURL, nil),
      let portrait = CGImageSourceCreateImageAtIndex(source, 0, nil),
      // The face in the 256 x 256 portrait, in image coordinates (origin top-left).
      let face = portrait.cropping(to: CGRect(x: 68, y: 62, width: 146, height: 146))
else {
    FileHandle.standardError.write(Data("make_icon: cannot read the portrait\n".utf8))
    exit(1)
}

func render(pixels: Int) -> CGImage? {
    guard let context = CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
    // Apple's icon grid: an 824/1024 rounded square, centred, with clear margins.
    let scale = CGFloat(pixels) / 1024
    let tile = CGRect(x: 100 * scale, y: 100 * scale, width: 824 * scale, height: 824 * scale)
    context.addPath(CGPath(roundedRect: tile, cornerWidth: 185 * scale, cornerHeight: 185 * scale, transform: nil))
    context.clip()
    context.setFillColor(CGColor(srgbRed: 0.06, green: 0.05, blue: 0.07, alpha: 1))
    context.fill(tile)
    context.interpolationQuality = .high
    context.draw(face, in: tile)
    return context.makeImage()
}

let folder = URL(fileURLWithPath: arguments[2], isDirectory: true)
try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
for points in [16, 32, 128, 256, 512] {
    for (scale, suffix) in [(1, ""), (2, "@2x")] {
        let url = folder.appendingPathComponent("icon_\(points)x\(points)\(suffix).png")
        guard let image = render(pixels: points * scale),
              let file = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)
        else { exit(1) }
        CGImageDestinationAddImage(file, image, nil)
        guard CGImageDestinationFinalize(file) else { exit(1) }
    }
}
