// Generates the OCR qualification fixtures (decision 0018): image-only PDFs
// (no text layer) with KNOWN text, plus one mixed PDF. Run:
//   swiftc -O tools/make_ocr_fixtures.swift -o /tmp/make_ocr && /tmp/make_ocr fixtures/ocr
import Foundation
import CoreGraphics
import CoreText

let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "fixtures/ocr"
let W = 1240, H = 1754  // A4 at 150 dpi

func drawLines(_ ctx: CGContext, _ lines: [String], fontName: String, size: CGFloat, rtl: Bool) {
    var y = CGFloat(H) - 160
    for line in lines {
        let font = CTFontCreateWithName(fontName as CFString, size, nil)
        let attrs: [NSAttributedString.Key: Any] = [
            NSAttributedString.Key(kCTFontAttributeName as String): font,
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(gray: 0, alpha: 1)]
        let ctl = CTLineCreateWithAttributedString(NSAttributedString(string: line, attributes: attrs))
        let width = CTLineGetTypographicBounds(ctl, nil, nil, nil)
        ctx.textPosition = CGPoint(x: rtl ? CGFloat(W) - 120 - CGFloat(width) : 120, y: y)
        CTLineDraw(ctl, ctx)
        y -= size * 1.9
    }
}

func bitmap(_ lines: [String], fontName: String, size: CGFloat, rtl: Bool) -> CGImage {
    let ctx = CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0,
                        space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
    ctx.setFillColor(CGColor(gray: 1, alpha: 1)); ctx.fill(CGRect(x: 0, y: 0, width: W, height: H))
    drawLines(ctx, lines, fontName: fontName, size: size, rtl: rtl)
    return ctx.makeImage()!
}

/// A "phone photo" of the page: rotated ~1.5 deg, downsampled to 55 %,
/// speckled with deterministic noise and slightly darkened.
func degraded(_ lines: [String]) -> CGImage {
    let sw = Int(Double(W) * 0.55), sh = Int(Double(H) * 0.55)
    let ctx = CGContext(data: nil, width: sw, height: sh, bitsPerComponent: 8, bytesPerRow: 0,
                        space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
    ctx.setFillColor(CGColor(gray: 0.93, alpha: 1)); ctx.fill(CGRect(x: 0, y: 0, width: sw, height: sh))
    ctx.interpolationQuality = .low
    ctx.translateBy(x: CGFloat(sw) / 2, y: CGFloat(sh) / 2)
    ctx.rotate(by: 0.026)
    ctx.translateBy(x: -CGFloat(sw) / 2, y: -CGFloat(sh) / 2)
    ctx.scaleBy(x: 0.55, y: 0.55)
    drawLines(ctx, lines, fontName: "Helvetica", size: 40, rtl: false)
    let px = ctx.data!.assumingMemoryBound(to: UInt8.self)
    var seed: UInt64 = 0x9E3779B97F4A7C15
    for i in 0..<(sw * sh) {
        seed = seed &* 6364136223846793005 &+ 1442695040888963407
        let n = Int((seed >> 33) % 41) - 20          // +-20 grey levels
        px[i] = UInt8(max(0, min(255, Int(px[i]) + n)))
    }
    return ctx.makeImage()!
}

func writePDF(_ name: String, pages: [(image: CGImage?, lines: [String], font: String, rtl: Bool)]) {
    let url = URL(fileURLWithPath: outDir).appendingPathComponent(name)
    var box = CGRect(x: 0, y: 0, width: 595, height: 842)
    let ctx = CGContext(url as CFURL, mediaBox: &box, nil)!
    for p in pages {
        ctx.beginPDFPage(nil)
        if let img = p.image {
            ctx.draw(img, in: box)  // scanned: pixels only, no text layer
        } else {
            ctx.saveGState(); ctx.scaleBy(x: 595.0 / CGFloat(W), y: 842.0 / CGFloat(H))
            drawLines(ctx, p.lines, fontName: p.font, size: 34, rtl: p.rtl)  // vector text layer
            ctx.restoreGState()
        }
        ctx.endPDFPage()
    }
    ctx.closePDF()
}

let en = ["SUPPLIER INVOICE",
          "Invoice INV-1001 from Acme Supplies",
          "Issued 2026-08-01, due 2026-08-31",
          "Amount 4,200.00 USD. Status: unpaid.",
          "Invoice INV-1002 from Northwind Parts",
          "Issued 2026-09-05, due 2026-10-05",
          "Amount 1,850.50 USD. Status: unpaid."]
let ar = ["فاتورة المورد رقم 1001",
          "المبلغ الإجمالي 4200 ريال",
          "تاريخ الاستحقاق 2026-08-31",
          "الحالة غير مدفوعة"]
let enPage2 = ["Invoice INV-1003 from Acme Supplies",
               "Issued 2026-09-20, due 2026-10-20",
               "Amount 900.00 USD. Status: unpaid."]

writePDF("scanned_invoice_en.pdf", pages: [(bitmap(en, fontName: "Helvetica", size: 40, rtl: false), en, "Helvetica", false)])
// The full five-invoice batch as a scan: number, parties, dates, amount and
// status on SEPARATE lines, one blank line between invoices.
let batch = [
    "Invoice INV-1001", "Acme Supplies", "Issued 2026-08-01, due 2026-08-31", "Amount 4,200.00 USD - unpaid", "",
    "Invoice INV-1002", "Northwind Parts", "Issued 2026-09-05, due 2026-10-05", "Amount 1,850.50 USD - unpaid", "",
    "Invoice INV-1003", "Acme Supplies", "Issued 2026-09-20, due 2026-10-20", "Amount 900.00 USD - unpaid", "",
    "Invoice INV-1004", "Delta Logistics", "Issued 2026-07-15, due 2026-08-14", "Amount 2,300.00 USD - paid on 2026-08-10", "",
    "Invoice INV-1005", "Delta Logistics", "Issued 2026-08-20, due 2026-09-19", "Amount 3,100.00 USD - unpaid"]
func batchImage() -> CGImage {
    let ctx = CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0,
                        space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
    ctx.setFillColor(CGColor(gray: 1, alpha: 1)); ctx.fill(CGRect(x: 0, y: 0, width: W, height: H))
    drawLines(ctx, batch, fontName: "Helvetica", size: 30, rtl: false)
    return ctx.makeImage()!
}
writePDF("scanned_invoice_batch.pdf", pages: [(batchImage(), batch, "Helvetica", false)])
writePDF("scanned_invoice_en_degraded.pdf", pages: [(degraded(en), en, "Helvetica", false)])
writePDF("scanned_invoice_ar.pdf", pages: [(bitmap(ar, fontName: "GeezaPro", size: 44, rtl: true), ar, "GeezaPro", true)])
writePDF("mixed_text_and_scan.pdf", pages: [
    (nil, en, "Helvetica", false),
    (bitmap(enPage2, fontName: "Helvetica", size: 40, rtl: false), enPage2, "Helvetica", false)])
let truth: [String: Any] = [
    "scanned_invoice_en.pdf": ["pages": [en]],
    "scanned_invoice_en_degraded.pdf": ["pages": [en]],
    "scanned_invoice_batch.pdf": ["pages": [batch]],
    "scanned_invoice_ar.pdf": ["pages": [ar]],
    "mixed_text_and_scan.pdf": ["pages": [en, enPage2], "sources": ["text_layer", "ocr"]],
]
let d = try! JSONSerialization.data(withJSONObject: truth, options: [.prettyPrinted, .sortedKeys])
try! d.write(to: URL(fileURLWithPath: outDir).appendingPathComponent("truth.json"))
print("wrote fixtures to \(outDir)")
