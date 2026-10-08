import AppKit

let canvas: CGFloat = 1024
let arguments = CommandLine.arguments
guard arguments.count == 2 else {
    FileHandle.standardError.write("usage: make-icon.swift <output.icns>\n".data(using: .utf8)!)
    exit(64)
}
let destination = URL(fileURLWithPath: arguments[1])
let output = FileManager.default.temporaryDirectory
    .appendingPathComponent(UUID().uuidString)
    .appendingPathComponent("AppIcon.iconset", isDirectory: true)
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: output.deletingLastPathComponent()) }

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(
        srgbRed: CGFloat((hex >> 16) & 0xFF) / 255,
        green: CGFloat((hex >> 8) & 0xFF) / 255,
        blue: CGFloat(hex & 0xFF) / 255,
        alpha: alpha
    )
}

func drawIcon(in ctx: CGContext) {
    let body = CGRect(x: 100, y: 100, width: 824, height: 824)
    let shape = CGPath(roundedRect: body, cornerWidth: 185, cornerHeight: 185, transform: nil)

    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 28, color: color(0x000000, 0.35))
    ctx.addPath(shape)
    ctx.setFillColor(color(0x1B2559))
    ctx.fillPath()
    ctx.restoreGState()

    ctx.saveGState()
    ctx.addPath(shape)
    ctx.clip()
    let background = CGGradient(
        colorsSpace: CGColorSpace(name: CGColorSpace.sRGB),
        colors: [color(0x2DD4BF), color(0x2563EB), color(0x1E1B4B)] as CFArray,
        locations: [0, 0.55, 1]
    )!
    ctx.drawLinearGradient(background, start: CGPoint(x: body.minX, y: body.maxY), end: CGPoint(x: body.maxX, y: body.minY), options: [])
    let glow = CGGradient(
        colorsSpace: CGColorSpace(name: CGColorSpace.sRGB),
        colors: [color(0xFFFFFF, 0.22), color(0xFFFFFF, 0)] as CFArray,
        locations: [0, 1]
    )!
    ctx.drawRadialGradient(glow, startCenter: CGPoint(x: 330, y: 820), startRadius: 0, endCenter: CGPoint(x: 330, y: 820), endRadius: 560, options: [])
    ctx.restoreGState()

    let center = CGPoint(x: 512, y: 512)
    let radius: CGFloat = 270
    let lineWidth: CGFloat = 64

    ctx.setLineCap(.round)
    ctx.setLineWidth(lineWidth)
    ctx.setStrokeColor(color(0xFFFFFF, 0.22))
    ctx.addArc(center: center, radius: radius, startAngle: 0, endAngle: .pi * 2, clockwise: false)
    ctx.strokePath()

    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -4), blur: 16, color: color(0x0B1033, 0.35))
    ctx.setStrokeColor(color(0xFFFFFF))
    let start = CGFloat.pi / 2
    ctx.addArc(center: center, radius: radius, startAngle: start, endAngle: start - .pi * 2 * 0.72, clockwise: true)
    ctx.strokePath()
    ctx.restoreGState()

    let eyeWidth: CGFloat = 300
    let eyeHeight: CGFloat = 176
    let eye = CGMutablePath()
    eye.move(to: CGPoint(x: center.x - eyeWidth / 2, y: center.y))
    eye.addQuadCurve(to: CGPoint(x: center.x + eyeWidth / 2, y: center.y), control: CGPoint(x: center.x, y: center.y + eyeHeight))
    eye.addQuadCurve(to: CGPoint(x: center.x - eyeWidth / 2, y: center.y), control: CGPoint(x: center.x, y: center.y - eyeHeight))
    eye.closeSubpath()

    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -4), blur: 16, color: color(0x0B1033, 0.35))
    ctx.addPath(eye)
    ctx.setFillColor(color(0xFFFFFF))
    ctx.fillPath()
    ctx.restoreGState()

    ctx.saveGState()
    ctx.addPath(eye)
    ctx.clip()
    let iris = CGRect(x: center.x - 62, y: center.y - 62, width: 124, height: 124)
    let irisGradient = CGGradient(
        colorsSpace: CGColorSpace(name: CGColorSpace.sRGB),
        colors: [color(0x2563EB), color(0x1E1B4B)] as CFArray,
        locations: [0, 1]
    )!
    ctx.addEllipse(in: iris)
    ctx.clip()
    ctx.drawLinearGradient(irisGradient, start: CGPoint(x: iris.minX, y: iris.maxY), end: CGPoint(x: iris.maxX, y: iris.minY), options: [])
    ctx.restoreGState()

    ctx.setFillColor(color(0xFFFFFF, 0.9))
    ctx.fillEllipse(in: CGRect(x: center.x + 10, y: center.y + 14, width: 30, height: 30))
}

func render(pixels: Int) throws -> Data {
    let ctx = CGContext(
        data: nil,
        width: pixels,
        height: pixels,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB)!,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
    ctx.interpolationQuality = .high
    ctx.scaleBy(x: CGFloat(pixels) / canvas, y: CGFloat(pixels) / canvas)
    drawIcon(in: ctx)
    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    return rep.representation(using: .png, properties: [:])!
}

for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let name = scale == 1 ? "icon_\(points)x\(points).png" : "icon_\(points)x\(points)@2x.png"
        try render(pixels: points * scale).write(to: output.appendingPathComponent(name))
    }
}

let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", "-o", destination.path, output.path]
try iconutil.run()
iconutil.waitUntilExit()
guard iconutil.terminationStatus == 0 else { exit(iconutil.terminationStatus) }
print("Wrote \(destination.path)")
