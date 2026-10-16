// Hands the OS-engine adapters (OCRHost.swift, ASRHost.swift - compiled into
// the Runner from native/apple/) to the Rust core. Dart asks for the C entry
// points' addresses over the "dev.harbor/native_adapters" channel and passes
// them to harbor_ocr_register_host / harbor_asr_register_host on the worker
// isolate. The addresses are taken from the Swift symbols directly (inside the
// host files), so no dlsym lookup can be defeated by dead-stripping.
import Foundation

/// `{ocr: [descriptor, recognize, free], asr: [descriptor, transcribe, free]}`.
/// Registration is refused by the core unless the adapter reports itself
/// on-device, so handing the addresses over grants nothing by itself.
func harborNativeAdapterAddresses() -> [String: [Int]] {
  return ["ocr": harborOcrHostAddresses(), "asr": harborAsrHostAddresses()]
}
