// Draws the app icon: glowing "beat bars" above a two-button foot controller.
//
//   swift packaging/icon.swift <out.png>
//
// Renders a 1024×1024 PNG following the macOS icon grid (824 pt tile, 100 pt margin).
import AppKit
import CoreGraphics

let size: CGFloat = 1024
let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icon_1024.png"

func rgb(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(
        red: CGFloat((hex >> 16) & 0xFF) / 255,
        green: CGFloat((hex >> 8) & 0xFF) / 255,
        blue: CGFloat(hex & 0xFF) / 255,
        alpha: alpha)
}

let space = CGColorSpace(name: CGColorSpace.sRGB)!
let ctx = CGContext(
    data: nil, width: Int(size), height: Int(size), bitsPerComponent: 8, bytesPerRow: 0,
    space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
// Top-left origin, like a design tool.
ctx.translateBy(x: 0, y: size)
ctx.scaleBy(x: 1, y: -1)

func gradient(_ colors: [CGColor], _ stops: [CGFloat]? = nil) -> CGGradient {
    CGGradient(colorsSpace: space, colors: colors as CFArray, locations: stops)!
}

func linear(_ path: CGPath, _ colors: [CGColor], from: CGPoint, to: CGPoint) {
    ctx.saveGState()
    ctx.addPath(path)
    ctx.clip()
    ctx.drawLinearGradient(gradient(colors), start: from, end: to, options: [])
    ctx.restoreGState()
}

// ---- Tile -------------------------------------------------------------------------------------
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
let tilePath = CGPath(roundedRect: tile, cornerWidth: 186, cornerHeight: 186, transform: nil)

ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: 14), blur: 30, color: rgb(0x000000, 0.45))
ctx.addPath(tilePath)
ctx.setFillColor(rgb(0x14172E))
ctx.fillPath()
ctx.restoreGState()

linear(tilePath, [rgb(0x262B57), rgb(0x12142A), rgb(0x0A0B18)],
       from: CGPoint(x: 0, y: tile.minY), to: CGPoint(x: 0, y: tile.maxY))

// Soft light from above.
ctx.saveGState()
ctx.addPath(tilePath)
ctx.clip()
ctx.drawRadialGradient(
    gradient([rgb(0x6C5CFF, 0.35), rgb(0x6C5CFF, 0)]),
    startCenter: CGPoint(x: 512, y: 230), startRadius: 0,
    endCenter: CGPoint(x: 512, y: 230), endRadius: 520, options: [])
ctx.restoreGState()

// Hairline rim.
ctx.saveGState()
ctx.addPath(CGPath(roundedRect: tile.insetBy(dx: 1.5, dy: 1.5), cornerWidth: 185, cornerHeight: 185, transform: nil))
ctx.setStrokeColor(rgb(0xFFFFFF, 0.10))
ctx.setLineWidth(3)
ctx.strokePath()
ctx.restoreGState()

// ---- Beat bars --------------------------------------------------------------------------------
let heights: [CGFloat] = [0.42, 0.74, 1.0, 0.58, 0.84]
let barWidth: CGFloat = 74
let gap: CGFloat = 30
let baseline: CGFloat = 548
let maxHeight: CGFloat = 330
let total = CGFloat(heights.count) * barWidth + CGFloat(heights.count - 1) * gap
var x = (size - total) / 2

for h in heights {
    let height = maxHeight * h
    let rect = CGRect(x: x, y: baseline - height, width: barWidth, height: height)
    let bar = CGPath(roundedRect: rect, cornerWidth: barWidth / 2, cornerHeight: barWidth / 2, transform: nil)
    // Glow.
    ctx.saveGState()
    ctx.setShadow(offset: .zero, blur: 46, color: rgb(0xFF3D8B, 0.75))
    ctx.addPath(bar)
    ctx.setFillColor(rgb(0xFF4D7A))
    ctx.fillPath()
    ctx.restoreGState()
    // Body: warm orange at the floor to hot pink at the top.
    linear(bar, [rgb(0xFF2E93), rgb(0xFF5A5F), rgb(0xFFA63D)],
           from: CGPoint(x: 0, y: baseline - maxHeight), to: CGPoint(x: 0, y: baseline))
    // Gloss on the left edge.
    let gloss = CGPath(roundedRect: CGRect(x: rect.minX + 12, y: rect.minY + 14, width: 16, height: max(rect.height - 28, 0)),
                       cornerWidth: 8, cornerHeight: 8, transform: nil)
    ctx.saveGState()
    ctx.addPath(gloss)
    ctx.setFillColor(rgb(0xFFFFFF, 0.28))
    ctx.fillPath()
    ctx.restoreGState()
    x += barWidth + gap
}

// ---- Foot controller --------------------------------------------------------------------------
let body = CGRect(x: 196, y: 612, width: 632, height: 214)
let bodyPath = CGPath(roundedRect: body, cornerWidth: 54, cornerHeight: 54, transform: nil)
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: 18), blur: 28, color: rgb(0x000000, 0.55))
ctx.addPath(bodyPath)
ctx.setFillColor(rgb(0x2A2E48))
ctx.fillPath()
ctx.restoreGState()
linear(bodyPath, [rgb(0x4A5078), rgb(0x2C3050), rgb(0x1C1F36)],
       from: CGPoint(x: 0, y: body.minY), to: CGPoint(x: 0, y: body.maxY))
// Top bevel.
ctx.saveGState()
ctx.addPath(CGPath(roundedRect: body.insetBy(dx: 2, dy: 2), cornerWidth: 52, cornerHeight: 52, transform: nil))
ctx.setStrokeColor(rgb(0xFFFFFF, 0.16))
ctx.setLineWidth(4)
ctx.strokePath()
ctx.restoreGState()

func footswitch(center c: CGPoint, lit: Bool) {
    let ledCenter = CGPoint(x: c.x, y: body.minY + 34)
    let led = CGRect(x: ledCenter.x - 13, y: ledCenter.y - 13, width: 26, height: 26)
    ctx.saveGState()
    if lit {
        ctx.setShadow(offset: .zero, blur: 30, color: rgb(0x3DF5C8, 0.95))
        ctx.setFillColor(rgb(0x5CFFD8))
    } else {
        ctx.setFillColor(rgb(0x0E1020))
    }
    ctx.fillEllipse(in: led)
    ctx.restoreGState()

    let r: CGFloat = 60
    let outer = CGRect(x: c.x - r, y: c.y - r, width: 2 * r, height: 2 * r)
    // Glow ring around the active switch.
    if lit {
        ctx.saveGState()
        ctx.setShadow(offset: .zero, blur: 40, color: rgb(0x3DF5C8, 0.8))
        ctx.setStrokeColor(rgb(0x3DF5C8, 0.9))
        ctx.setLineWidth(7)
        ctx.strokeEllipse(in: outer.insetBy(dx: -12, dy: -12))
        ctx.restoreGState()
    }
    // Chrome cap.
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: 8), blur: 12, color: rgb(0x000000, 0.6))
    ctx.setFillColor(rgb(0x9AA0B8))
    ctx.fillEllipse(in: outer)
    ctx.restoreGState()
    linear(CGPath(ellipseIn: outer, transform: nil), [rgb(0xF4F6FF), rgb(0xA9AFC6), rgb(0x6A7090)],
           from: CGPoint(x: 0, y: outer.minY), to: CGPoint(x: 0, y: outer.maxY))
    let inner = outer.insetBy(dx: 16, dy: 16)
    linear(CGPath(ellipseIn: inner, transform: nil), [rgb(0x7D839F), rgb(0xC9CEE0), rgb(0xEEF0F8)],
           from: CGPoint(x: 0, y: inner.minY), to: CGPoint(x: 0, y: inner.maxY))
}

footswitch(center: CGPoint(x: 384, y: 736), lit: true)
footswitch(center: CGPoint(x: 640, y: 736), lit: false)

// ---- Write ------------------------------------------------------------------------------------
let image = ctx.makeImage()!
let rep = NSBitmapImageRep(cgImage: image)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("wrote \(out)")
