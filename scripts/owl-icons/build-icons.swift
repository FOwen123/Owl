// Builds every Owl logo asset from the hand-drawn source (owl-source.png).
//
//   swift scripts/owl-icons/build-icons.swift
//   bun run tauri icon /tmp/owl-app-icon.png
//
// Writes: src/assets/owl-mark.png (in-app logo, transparent outside, white
// eyes), src-tauri/resources/*.png (menu-bar icons) and /tmp/owl-app-icon.png
// (feed it to `tauri icon`).
import AppKit

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
func path(_ p: String) -> String { root.appendingPathComponent(p).path }

// --- Load the source as a grayscale-ish RGBA buffer --------------------------
let source = NSBitmapImageRep(data: NSImage(contentsOfFile: path("scripts/owl-icons/owl-source.png"))!.tiffRepresentation!)!
let w = source.pixelsWide, h = source.pixelsHigh
var ink = [Bool](repeating: false, count: w * h)  // dark brush pixels
for y in 0..<h { for x in 0..<w {
    let c = source.colorAt(x: x, y: y)!.usingColorSpace(.deviceRGB)!
    ink[y * w + x] = (c.redComponent + c.greenComponent + c.blueComponent) / 3 < 0.5
} }

// Paper outside the owl = light pixels reachable from the border. Light pixels
// inside (eye whites, highlights) stay white.
var outside = [Bool](repeating: false, count: w * h)
var stack: [Int] = []
for x in 0..<w { stack += [x, (h - 1) * w + x] }
for y in 0..<h { stack += [y * w, y * w + w - 1] }
while let i = stack.popLast() {
    if outside[i] || ink[i] { continue }
    outside[i] = true
    let x = i % w, y = i / w
    if x > 0 { stack.append(i - 1) }; if x < w - 1 { stack.append(i + 1) }
    if y > 0 { stack.append(i - w) }; if y < h - 1 { stack.append(i + w) }
}

// One solid pupil per eye: fill the small highlight dots inside the eyes.
// The two largest light areas inside the owl are the eye whites; any smaller
// light area within their bounds (a highlight) becomes ink. The beak sits
// outside the eyes, so it stays white.
struct Region { var pixels: [Int]; var minX: Int, minY: Int, maxX: Int, maxY: Int }
var seen = outside
var regions: [Region] = []
for start in 0..<(w * h) where !seen[start] && !ink[start] {
    var r = Region(pixels: [], minX: w, minY: h, maxX: 0, maxY: 0)
    stack = [start]
    while let i = stack.popLast() {
        if seen[i] || ink[i] { continue }
        seen[i] = true
        let x = i % w, y = i / w
        r.pixels.append(i)
        r.minX = min(r.minX, x); r.maxX = max(r.maxX, x); r.minY = min(r.minY, y); r.maxY = max(r.maxY, y)
        if x > 0 { stack.append(i - 1) }; if x < w - 1 { stack.append(i + 1) }
        if y > 0 { stack.append(i - w) }; if y < h - 1 { stack.append(i + w) }
    }
    regions.append(r)
}
regions.sort { $0.pixels.count > $1.pixels.count }
let eyes = regions.prefix(2)
for r in regions.dropFirst(2) where eyes.contains(where: {
    r.minX >= $0.minX && r.maxX <= $0.maxX && r.minY >= $0.minY && r.maxY <= $0.maxY
}) {
    for i in r.pixels { ink[i] = true }
}

// Each whole eye (white plus pupil): what a flood from the edge of the eye's
// box can't reach without crossing the eye white. Lets the overlay move the
// pupils and close the lids (RecordingOverlay).
var eyeHole = [Bool](repeating: false, count: w * h)
for e in eyes {
    let white = Set(e.pixels)
    let x0 = max(e.minX - 2, 0), x1 = min(e.maxX + 2, w - 1)
    let y0 = max(e.minY - 2, 0), y1 = min(e.maxY + 2, h - 1)
    var reached = Set<Int>()
    stack = []
    for x in x0...x1 { stack += [y0 * w + x, y1 * w + x] }
    for y in y0...y1 { stack += [y * w + x0, y * w + x1] }
    while let i = stack.popLast() {
        let x = i % w, y = i / w
        if x < x0 || x > x1 || y < y0 || y > y1 || reached.contains(i) || white.contains(i) { continue }
        reached.insert(i)
        stack += [i - 1, i + 1, i - w, i + w]
    }
    for y in y0...y1 { for x in x0...x1 where !reached.contains(y * w + x) { eyeHole[y * w + x] = true } }
}
// The pupils: the two largest ink blobs in the eyes. Stray brush specks along
// the eye edges stay on the body, so they don't slide when the owl glances.
var blobs: [[Int]] = []
var visited = [Bool](repeating: false, count: w * h)
for start in 0..<(w * h) where eyeHole[start] && ink[start] && !visited[start] {
    var blob: [Int] = []
    stack = [start]
    while let i = stack.popLast() {
        if visited[i] || !eyeHole[i] || !ink[i] { continue }
        visited[i] = true
        blob.append(i)
        stack += [i - 1, i + 1, i - w, i + w]
    }
    blobs.append(blob)
}
var pupil = [Bool](repeating: false, count: w * h)
for blob in blobs.sorted(by: { $0.count > $1.count }).prefix(2) { for i in blob { pupil[i] = true } }

// Square crop around the owl with a little breathing room.
var minX = w, minY = h, maxX = 0, maxY = 0
for y in 0..<h { for x in 0..<w where !outside[y * w + x] {
    minX = min(minX, x); maxX = max(maxX, x); minY = min(minY, y); maxY = max(maxY, y)
} }
let side = Int(Double(max(maxX - minX, maxY - minY)) * 1.06)
let cx = (minX + maxX) / 2, cy = (minY + maxY) / 2

/// Renders the cropped owl. `inkColor` for brush strokes, `paper` for the eye
/// whites (nil = transparent holes, for menu-bar templates).
/// Draw the result with `.draw(in:)`; it scales smoothly to any size.
func owl(ink inkColor: NSColor, paper: NSColor?) -> NSBitmapImageRep {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: side, pixelsHigh: side,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: side * 4, bitsPerPixel: 32)!
    // Raw premultiplied RGBA bytes (row 0 is the top of the image).
    func bytes(_ c: NSColor?) -> [UInt8] {
        guard let c = c?.usingColorSpace(.deviceRGB), c.alphaComponent > 0 else { return [0, 0, 0, 0] }
        return [c.redComponent, c.greenComponent, c.blueComponent, 1].map { UInt8($0 * 255) }
    }
    let inkPx = bytes(inkColor), paperPx = bytes(paper), clearPx = bytes(nil)
    let data = rep.bitmapData!
    for y in 0..<side { for x in 0..<side {
        let sx = cx - side / 2 + x, sy = cy - side / 2 + y
        var px = clearPx
        if sx >= 0, sy >= 0, sx < w, sy < h {
            let i = sy * w + sx
            px = ink[i] ? inkPx : (outside[i] ? clearPx : paperPx)
        }
        for k in 0..<4 { data[(y * side + x) * 4 + k] = px[k] }
    } }
    return rep
}

extension NSBitmapImageRep {
    /// draw(in:) copies (clearing what's underneath); this blends on top.
    func over(_ rect: NSRect) {
        draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
    }
}

func canvas(_ size: Int, _ draw: (NSRect) -> Void) -> NSBitmapImageRep {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    NSGraphicsContext.current?.imageInterpolation = .high
    draw(NSRect(x: 0, y: 0, width: size, height: size))
    NSGraphicsContext.restoreGraphicsState()
    return rep
}

func save(_ rep: NSBitmapImageRep, _ p: String) {
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: p))
    print("wrote", p)
}

/// One layer of the cropped owl as a white-on-transparent mask.
func layer(_ include: (Int) -> Bool) -> NSBitmapImageRep {
    let rep = owl(ink: .white, paper: nil)
    let data = rep.bitmapData!
    for y in 0..<side { for x in 0..<side {
        let sx = cx - side / 2 + x, sy = cy - side / 2 + y
        if sx < 0 || sy < 0 || sx >= w || sy >= h || !include(sy * w + sx) {
            for k in 0..<4 { data[(y * side + x) * 4 + k] = 0 }
        } else {
            for k in 0..<4 { data[(y * side + x) * 4 + k] = 255 }
        }
    } }
    return rep
}

// --- Overlay owl: the menu-bar mascot in three masks, so it can blink and look around.
let parts = path("src/overlay/owl")
try! FileManager.default.createDirectory(atPath: parts, withIntermediateDirectories: true)
save(canvas(128) { r in layer { ink[$0] && !pupil[$0] }.over(r) }, "\(parts)/body.png")
// The eye mask reaches a little past the eye edge, so a closed lid overlaps
// the body instead of leaving a faint seam.
var eyeMask = eyeHole
for _ in 0..<12 {
    let prev = eyeMask
    for i in (w + 1)..<(w * h - w - 1) where !prev[i] && (prev[i - 1] || prev[i + 1] || prev[i - w] || prev[i + w]) {
        eyeMask[i] = true
    }
}
save(canvas(128) { r in layer { eyeMask[$0] }.over(r) }, "\(parts)/eyes.png")
save(canvas(128) { r in layer { pupil[$0] }.over(r) }, "\(parts)/pupils.png")

// --- In-app mark ---------------------------------------------------------------
save(canvas(512) { r in owl(ink: .black, paper: .white).over(r) }, path("src/assets/owl-mark.png"))

// --- App icon: the owl on a white macOS tile --------------------------------------
save(canvas(1024) { _ in
    let tile = NSBezierPath(roundedRect: NSRect(x: 100, y: 100, width: 824, height: 824), xRadius: 185, yRadius: 185)
    NSColor(white: 0.98, alpha: 1).setFill(); tile.fill()
    owl(ink: .black, paper: .white).over(NSRect(x: 157, y: 145, width: 710, height: 710))
}, "/tmp/owl-app-icon.png")

// --- Menu-bar icons (64 px). Blue badges mark the state: solid dot = recording,
// ring = transcribing, "!" = warning. -------------------------------------------------
enum Badge { case none, dot, ring, warning }
func tray(_ color: NSColor, _ badge: Badge, paper: NSColor? = nil) -> NSBitmapImageRep {
    canvas(64) { _ in
        let owlSize: CGFloat = badge == .none ? 64 : 54
        owl(ink: color, paper: paper).over(NSRect(x: 0, y: 64 - owlSize, width: owlSize, height: owlSize))
        let b = NSRect(x: 40, y: 0, width: 24, height: 24)
        // Status badges are always Owl blue (DESIGN.md accent, dark value).
        let blue = NSColor(srgbRed: 0x4c / 255, green: 0x8d / 255, blue: 0xff / 255, alpha: 1)
        blue.setFill(); blue.setStroke()
        switch badge {
        case .none: break
        case .dot: NSBezierPath(ovalIn: b).fill()
        case .ring:
            let ring = NSBezierPath(ovalIn: b.insetBy(dx: 3, dy: 3)); ring.lineWidth = 5; ring.stroke()
        case .warning:
            NSBezierPath(ovalIn: b).fill()
            NSGraphicsContext.current?.compositingOperation = .clear
            NSBezierPath(rect: NSRect(x: 50, y: 10, width: 4, height: 10)).fill()
            NSBezierPath(rect: NSRect(x: 50, y: 4, width: 4, height: 4)).fill()
        }
    }
}
let res = path("src-tauri/resources")
for (name, badge) in [("tray_idle", Badge.none), ("tray_recording", .dot), ("tray_transcribing", .ring), ("tray_idle_warning", .warning)] {
    save(tray(.white, badge), "\(res)/\(name).png")       // for dark menu bars
    save(tray(.black, badge), "\(res)/\(name)_dark.png")  // for light menu bars
}
// "Colored" tray theme: the full-colour mark.
for (name, badge) in [("owl", Badge.none), ("recording", .dot), ("transcribing", .ring), ("owl_warning", .warning)] {
    save(tray(.black, badge, paper: .white), "\(res)/\(name).png")
}
