// Live capture for the composer: record the microphone, take a photo.
// Channel "dev.harbor/capture" (Dart: lib/services/capture.dart). Files are
// written to the temporary directory and handed straight to the core's OCR /
// speech reads, which the Dart side then deletes. Nothing here uploads or
// keeps anything, and a permission refusal is reported as code "permission".
import AVFoundation
import Foundation
#if os(iOS)
import Flutter
import UIKit
#else
import FlutterMacOS
#endif

final class HarborCapture: NSObject {
  private var recorder: AVAudioRecorder?
  private var recordingURL: URL?
  /// Bumped by every start and cancel. A start that finishes after it was
  /// cancelled (or superseded) must not leave the microphone open.
  private var generation = 0
  #if os(iOS)
  private var photoResult: ((Any?) -> Void)?
  #endif

  /// What this platform can do right now.
  func support() -> [String: Bool] {
    #if os(iOS)
    return ["record": true, "photo": UIImagePickerController.isSourceTypeAvailable(.camera)]
    #else
    return ["record": true, "photo": false]
    #endif
  }

  func handle(_ method: String, result: @escaping (Any?) -> Void) {
    switch method {
    case "support": result(support())
    case "recordStart": recordStart(result)
    case "recordStop": recordStop(result)
    case "recordCancel": recordCancel(); result(nil)
    case "takePhoto": takePhoto(result)
    default: result(FlutterMethodNotImplemented)
    }
  }

  // MARK: microphone

  /// Ask for the microphone only when the person has not decided yet: calling
  /// the request API when permission is already settled can leave the
  /// completion handler uncalled (seen on the iOS 26 simulator), which hung
  /// the recording flow.
  private func requestMic(_ done: @escaping (Bool) -> Void) {
    #if os(iOS)
    if #available(iOS 17.0, *) {
      switch AVAudioApplication.shared.recordPermission {
      case .granted: done(true)
      case .denied: done(false)
      default:
        AVAudioApplication.requestRecordPermission { ok in DispatchQueue.main.async { done(ok) } }
      }
    } else {
      let session = AVAudioSession.sharedInstance()
      switch session.recordPermission {
      case .granted: done(true)
      case .denied: done(false)
      default:
        session.requestRecordPermission { ok in DispatchQueue.main.async { done(ok) } }
      }
    }
    #else
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized: done(true)
    case .notDetermined:
      AVCaptureDevice.requestAccess(for: .audio) { ok in DispatchQueue.main.async { done(ok) } }
    default: done(false)
    }
    #endif
  }

  /// Audio setup runs off the main thread: the audio subsystem can stall (it
  /// did, in the AAC encoder, on the iOS simulator) and the UI must not.
  private let audioQueue = DispatchQueue(label: "dev.harbor.capture.audio", qos: .userInitiated)

  private func recordStart(_ result: @escaping (Any?) -> Void) {
    recordCancel()
    let mine = generation
    requestMic { granted in
      guard granted else {
        result(FlutterError(code: "permission", message: "microphone access denied", details: nil))
        return
      }
      self.audioQueue.async {
        let outcome: Any?
        do {
          #if os(iOS)
          let session = AVAudioSession.sharedInstance()
          try session.setCategory(.record, mode: .measurement)
          try session.setActive(true)
          #endif
          let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("harbor-dictation-\(UUID().uuidString).wav")
          // 16 kHz mono 16-bit PCM in a WAV: what the speech engines read, and
          // no encoder in the path.
          let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatLinearPCM,
            AVSampleRateKey: 16_000,
            AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
          ]
          let r = try AVAudioRecorder(url: url, settings: settings)
          if r.record() {
            var stale = false
            DispatchQueue.main.sync {
              stale = self.generation != mine
              if !stale {
                self.recorder = r
                self.recordingURL = url
              }
            }
            if stale {
              r.stop()
              try? FileManager.default.removeItem(at: url)
              outcome = FlutterError(code: "cancelled", message: "the recording was cancelled", details: nil)
            } else {
              outcome = nil
            }
          } else {
            outcome = FlutterError(code: "record", message: "the recorder could not start", details: nil)
          }
        } catch {
          outcome = FlutterError(code: "record", message: "\(error)", details: nil)
        }
        DispatchQueue.main.async { result(outcome) }
      }
    }
  }

  private func recordStop(_ result: @escaping (Any?) -> Void) {
    guard let r = recorder, let url = recordingURL else {
      result(FlutterError(code: "empty", message: "nothing is recording", details: nil))
      return
    }
    r.stop()
    recorder = nil
    recordingURL = nil
    #if os(iOS)
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    #endif
    result(url.path)
  }

  private func recordCancel() {
    generation += 1
    guard let r = recorder else { return }
    r.stop()
    if let url = recordingURL { try? FileManager.default.removeItem(at: url) }
    recorder = nil
    recordingURL = nil
    #if os(iOS)
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    #endif
  }

  // MARK: camera (iOS)

  private func takePhoto(_ result: @escaping (Any?) -> Void) {
    #if os(iOS)
    guard UIImagePickerController.isSourceTypeAvailable(.camera) else {
      result(FlutterError(code: "unsupported", message: "no camera on this device", details: nil))
      return
    }
    func present() {
      guard let top = HarborCapture.topViewController() else {
        result(FlutterError(code: "photo", message: "no window to present the camera", details: nil))
        return
      }
      let picker = UIImagePickerController()
      picker.sourceType = .camera
      picker.delegate = self
      photoResult = result
      top.present(picker, animated: true)
    }
    switch AVCaptureDevice.authorizationStatus(for: .video) {
    case .authorized: present()
    case .notDetermined:
      AVCaptureDevice.requestAccess(for: .video) { ok in
        DispatchQueue.main.async {
          ok ? present() : result(FlutterError(code: "permission", message: "camera access denied", details: nil))
        }
      }
    default:
      result(FlutterError(code: "permission", message: "camera access denied", details: nil))
    }
    #else
    result(FlutterError(code: "unsupported", message: "no camera capture on this platform", details: nil))
    #endif
  }

  #if os(iOS)
  static func topViewController() -> UIViewController? {
    let scene = UIApplication.shared.connectedScenes
      .compactMap { $0 as? UIWindowScene }.first { $0.activationState == .foregroundActive }
    var top = scene?.windows.first { $0.isKeyWindow }?.rootViewController
    while let presented = top?.presentedViewController { top = presented }
    return top
  }
  #endif
}

#if os(iOS)
extension HarborCapture: UIImagePickerControllerDelegate, UINavigationControllerDelegate {
  func imagePickerController(
    _ picker: UIImagePickerController,
    didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]
  ) {
    let done = photoResult
    photoResult = nil
    picker.dismiss(animated: true)
    guard let image = info[.originalImage] as? UIImage, let jpeg = image.jpegData(compressionQuality: 0.9)
    else {
      done?(FlutterError(code: "photo", message: "the photo could not be read", details: nil))
      return
    }
    let url = FileManager.default.temporaryDirectory
      .appendingPathComponent("harbor-photo-\(UUID().uuidString).jpg")
    do {
      try jpeg.write(to: url)
      done?(url.path)
    } catch {
      done?(FlutterError(code: "photo", message: "\(error)", details: nil))
    }
  }

  func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
    let done = photoResult
    photoResult = nil
    picker.dismiss(animated: true)
    done?(nil)  // cancelled: Dart sees null, not an error
  }
}
#endif
