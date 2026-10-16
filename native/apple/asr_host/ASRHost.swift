// Apple on-device speech adapter (decision 0019): SpeechTranscriber /
// SpeechAnalyzer, the OS's own embedded model - no cloud, no third party.
import Foundation
import Speech
import AVFoundation

private func cstr(_ obj: [String: Any]) -> UnsafeMutablePointer<CChar>? {
    guard let d = try? JSONSerialization.data(withJSONObject: obj, options: [.sortedKeys]),
          let s = String(data: d, encoding: .utf8) else { return nil }
    return strdup(s)
}

/// Run an async body to completion from the synchronous C ABI.
private func blocking<T>(_ body: @escaping @Sendable () async -> T) -> T {
    let box = Box<T>()
    let sem = DispatchSemaphore(value: 0)
    Task.detached { box.value = await body(); sem.signal() }
    sem.wait()
    return box.value!
}
private final class Box<T>: @unchecked Sendable { var value: T? }

// SpeechTranscriber needs iOS / macOS 26. The Runner's deployment target is
// lower, so the implementation is gated and the exported entry points answer
// "unsupported" (code 1) on older systems instead of failing to link or crash.
@available(iOS 26.0, macOS 26.0, *)
private func asrDescriptor() -> UnsafeMutablePointer<CChar>? {
    let (supported, installed): ([String], [String]) = blocking {
        (await SpeechTranscriber.supportedLocales.map { $0.identifier }.sorted(),
         await SpeechTranscriber.installedLocales.map { $0.identifier }.sorted())
    }
    let v = ProcessInfo.processInfo.operatingSystemVersion
    #if arch(arm64)
    let arch = "arm64"
    #else
    let arch = "x86_64"
    #endif
    return cstr([
        "schema": "harbor.asr_host/v1", "engine": "apple-speech-transcriber",
        "execution_location": "on_device",
        "supported_locales": supported, "installed_locales": installed,
        "identity": ["os": "\(v.majorVersion).\(v.minorVersion).\(v.patchVersion)", "arch": arch],
    ])
}

@available(iOS 26.0, macOS 26.0, *)
private func asrTranscribe(
    _ requestJSON: UnsafePointer<CChar>?, _ data: UnsafePointer<UInt8>?, _ len: Int,
    _ cancel: UnsafePointer<Bool>?, _ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> Int32 {
    func fail(_ code: Int32, _ msg: String) -> Int32 { out?.pointee = cstr(["error": msg]); return code }
    guard let requestJSON, let data, len > 0, let out else { return fail(4, "empty request or audio") }
    guard let req = (try? JSONSerialization.jsonObject(with: Data(bytes: requestJSON, count: strlen(requestJSON))))
            as? [String: Any] else { return fail(4, "request is not JSON") }
    let localeId = (req["locale"] as? String) ?? "en_US"
    let audio = Data(bytes: data, count: len)

    let tmp = FileManager.default.temporaryDirectory.appendingPathComponent("harbor-asr-\(UUID().uuidString)")
    do { try audio.write(to: tmp) } catch { return fail(4, "cannot stage audio: \(error)") }
    defer { try? FileManager.default.removeItem(at: tmp) }

    struct Outcome { var code: Int32; var json: [String: Any] }
    let outcome: Outcome = blocking {
        let locale = Locale(identifier: localeId)
        let supported = await SpeechTranscriber.supportedLocales.map { $0.identifier(.bcp47) }
        guard supported.contains(locale.identifier(.bcp47)) else {
            return Outcome(code: 1, json: ["error": "locale \(localeId) is not supported by the on-device speech model"])
        }
        let installed = await SpeechTranscriber.installedLocales.map { $0.identifier(.bcp47) }
        guard installed.contains(locale.identifier(.bcp47)) else {
            return Outcome(code: 1, json: ["error": "the on-device speech model for \(localeId) is not installed on this device"])
        }
        do {
            let transcriber = SpeechTranscriber(locale: locale, transcriptionOptions: [],
                                                reportingOptions: [], attributeOptions: [.audioTimeRange])
            let analyzer = SpeechAnalyzer(modules: [transcriber])
            let file = try AVAudioFile(forReading: tmp)
            async let collected: [[String: Any]] = {
                var segs: [[String: Any]] = []
                for try await r in transcriber.results {
                    var start = 0.0, end = 0.0
                    for run in r.text.runs {
                        if let range = run.audioTimeRange {
                            start = min(start == 0 ? range.start.seconds : start, range.start.seconds)
                            end = max(end, range.end.seconds)
                        }
                    }
                    segs.append(["start": start, "end": end, "text": String(r.text.characters)])
                }
                return segs
            }()
            if let last = try await analyzer.analyzeSequence(from: file) {
                try await analyzer.finalizeAndFinish(through: last)
            } else {
                await analyzer.cancelAndFinishNow()
            }
            let segs = try await collected
            let text = segs.map { $0["text"] as? String ?? "" }.joined().trimmingCharacters(in: .whitespacesAndNewlines)
            return Outcome(code: 0, json: ["engine": "apple-speech-transcriber", "locale": localeId, "text": text, "segments": segs])
        } catch {
            return Outcome(code: 4, json: ["error": "transcription failed: \(error)"])
        }
    }
    if cancel?.pointee == true { return fail(3, "cancelled") }
    out.pointee = cstr(outcome.json)
    return outcome.code
}

@_cdecl("harbor_asr_host_descriptor")
public func harbor_asr_host_descriptor() -> UnsafeMutablePointer<CChar>? {
    if #available(iOS 26.0, macOS 26.0, *) { return asrDescriptor() }
    return nil  // the registrar reports "host returned no descriptor"
}

@_cdecl("harbor_asr_host_transcribe")
public func harbor_asr_host_transcribe(
    _ requestJSON: UnsafePointer<CChar>?, _ data: UnsafePointer<UInt8>?, _ len: Int,
    _ cancel: UnsafePointer<Bool>?, _ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> Int32 {
    if #available(iOS 26.0, macOS 26.0, *) {
        return asrTranscribe(requestJSON, data, len, cancel, out)
    }
    out?.pointee = cstr(["error": "on-device speech recognition needs iOS or macOS 26"])
    return 1
}

@_cdecl("harbor_asr_host_free_string")
public func harbor_asr_host_free_string(_ s: UnsafeMutablePointer<CChar>?) { free(s) }

/// Addresses of this adapter's C entry points (descriptor, transcribe, free_string),
/// taken here - in the file that defines them - for the Runner's
/// native_adapters channel. Taking them from another file makes Swift emit a
/// second C-named thunk and the link fails with duplicate symbols.
public func harborAsrHostAddresses() -> [Int] {
    typealias Descriptor = @convention(c) () -> UnsafeMutablePointer<CChar>?
    typealias Run = @convention(c) (
        UnsafePointer<CChar>?, UnsafePointer<UInt8>?, Int, UnsafePointer<Bool>?,
        UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
    ) -> Int32
    typealias Free = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void
    let d: Descriptor = harbor_asr_host_descriptor
    let r: Run = harbor_asr_host_transcribe
    let f: Free = harbor_asr_host_free_string
    return [unsafeBitCast(d, to: Int.self), unsafeBitCast(r, to: Int.self), unsafeBitCast(f, to: Int.self)]
}
