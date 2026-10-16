// Apple Vision OCR adapter (decision 0018). On-device only; see
// native/apple/include/harbor_ocr_host.h for the contract.
import Foundation
import PDFKit
import Vision
import ImageIO
import CoreGraphics

private func cstr(_ obj: [String: Any]) -> UnsafeMutablePointer<CChar>? {
    guard let d = try? JSONSerialization.data(withJSONObject: obj, options: [.sortedKeys]),
          let s = String(data: d, encoding: .utf8) else { return nil }
    return strdup(s)
}

private func recognize(_ image: CGImage, languages: [String]) throws -> (String, Double) {
    let req = VNRecognizeTextRequest()
    req.recognitionLevel = .accurate
    req.usesLanguageCorrection = true
    if !languages.isEmpty {
        req.recognitionLanguages = languages
    } else if #available(macOS 13.0, iOS 16.0, *) {
        req.automaticallyDetectsLanguage = true
    }
    try VNImageRequestHandler(cgImage: image, options: [:]).perform([req])
    let obs = (req.results ?? [])
    // Reading order: top to bottom, then left to right (Vision's origin is
    // bottom-left). Right-to-left lines keep Vision's own character order.
    let sorted = obs.sorted {
        let dy = $0.boundingBox.midY - $1.boundingBox.midY
        if abs(dy) > 0.012 { return dy > 0 }
        return $0.boundingBox.minX < $1.boundingBox.minX
    }
    var lines: [String] = []
    var conf = 0.0
    for o in sorted {
        if let c = o.topCandidates(1).first {
            lines.append(c.string)
            conf += Double(c.confidence)
        }
    }
    return (lines.joined(separator: "\n"), obs.isEmpty ? 0.0 : conf / Double(obs.count))
}

private func render(_ page: PDFPage, scale: CGFloat) -> CGImage? {
    let box = page.bounds(for: .mediaBox)
    let w = Int(box.width * scale), h = Int(box.height * scale)
    guard w > 0, h > 0, w < 12000, h < 12000,
          let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
                              space: CGColorSpaceCreateDeviceRGB(),
                              bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
    ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
    ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
    ctx.scaleBy(x: scale, y: scale)
    page.draw(with: .mediaBox, to: ctx)
    return ctx.makeImage()
}

@_cdecl("harbor_ocr_host_descriptor")
public func harbor_ocr_host_descriptor() -> UnsafeMutablePointer<CChar>? {
    var langs: [String] = []
    let req = VNRecognizeTextRequest()
    req.recognitionLevel = .accurate
    if let l = try? req.supportedRecognitionLanguages() { langs = l }
    let v = ProcessInfo.processInfo.operatingSystemVersion
    #if arch(arm64)
    let arch = "arm64"
    #else
    let arch = "x86_64"
    #endif
    return cstr([
        "schema": "harbor.ocr_host/v1", "engine": "apple-vision",
        "execution_location": "on_device", "languages": langs,
        "identity": ["os": "\(v.majorVersion).\(v.minorVersion).\(v.patchVersion)", "arch": arch],
    ])
}

@_cdecl("harbor_ocr_host_recognize")
public func harbor_ocr_host_recognize(
    _ requestJSON: UnsafePointer<CChar>?, _ data: UnsafePointer<UInt8>?, _ len: Int,
    _ cancel: UnsafePointer<Bool>?, _ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> Int32 {
    func fail(_ code: Int32, _ msg: String) -> Int32 {
        out?.pointee = cstr(["error": msg]); return code
    }
    guard let requestJSON, let data, len > 0, let out else { return fail(4, "empty request or data") }
    guard let req = (try? JSONSerialization.jsonObject(with: Data(bytes: requestJSON, count: strlen(requestJSON))))
            as? [String: Any] else { return fail(4, "request is not JSON") }
    let kind = req["kind"] as? String ?? ""
    let languages = (req["languages"] as? [String]) ?? []
    let maxPages = max(1, min((req["max_pages"] as? Int) ?? 50, 200))
    let bytes = Data(bytes: data, count: len)
    var pages: [[String: Any]] = []
    do {
        if kind == "pdf" {
            guard let doc = PDFDocument(data: bytes) else { return fail(4, "not a readable PDF") }
            if doc.isLocked { return fail(4, "PDF is password protected") }
            for i in 0..<min(doc.pageCount, maxPages) {
                if cancel?.pointee == true { return fail(3, "cancelled") }
                guard let page = doc.page(at: i) else { continue }
                let layer = (page.string ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
                if layer.count >= 20 {
                    pages.append(["page": i + 1, "text": layer, "source": "text_layer", "confidence": 1.0])
                    continue
                }
                guard let img = render(page, scale: 3.0) else { return fail(4, "page \(i + 1) could not be rendered") }
                let (t, c) = try recognize(img, languages: languages)
                pages.append(["page": i + 1, "text": t, "source": "ocr", "confidence": c])
            }
        } else if kind == "image" {
            guard let src = CGImageSourceCreateWithData(bytes as CFData, nil),
                  let img = CGImageSourceCreateImageAtIndex(src, 0, nil) else { return fail(4, "not a readable image") }
            let (t, c) = try recognize(img, languages: languages)
            pages.append(["page": 1, "text": t, "source": "ocr", "confidence": c])
        } else {
            return fail(1, "kind must be pdf or image")
        }
    } catch {
        return fail(4, "recognition failed: \(error)")
    }
    out.pointee = cstr(["engine": "apple-vision", "pages": pages])
    return 0
}

@_cdecl("harbor_ocr_host_free_string")
public func harbor_ocr_host_free_string(_ s: UnsafeMutablePointer<CChar>?) { free(s) }

/// Addresses of this adapter's C entry points (descriptor, recognize, free_string),
/// taken here - in the file that defines them - for the Runner's
/// native_adapters channel. Taking them from another file makes Swift emit a
/// second C-named thunk and the link fails with duplicate symbols.
public func harborOcrHostAddresses() -> [Int] {
    typealias Descriptor = @convention(c) () -> UnsafeMutablePointer<CChar>?
    typealias Run = @convention(c) (
        UnsafePointer<CChar>?, UnsafePointer<UInt8>?, Int, UnsafePointer<Bool>?,
        UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
    ) -> Int32
    typealias Free = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void
    let d: Descriptor = harbor_ocr_host_descriptor
    let r: Run = harbor_ocr_host_recognize
    let f: Free = harbor_ocr_host_free_string
    return [unsafeBitCast(d, to: Int.self), unsafeBitCast(r, to: Int.self), unsafeBitCast(f, to: Int.self)]
}
