import AVFoundation
import Cocoa
import FlutterMacOS
import ImageIO
import UniformTypeIdentifiers

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)

    // Video -> sampled JPEG frames for multimodal indexing (the Rust core
    // carries no video decoder). Dart: lib/services/video_frames.dart.
    FlutterMethodChannel(
      name: "dev.harbor.video_frames",
      binaryMessenger: flutterViewController.engine.binaryMessenger
    ).setMethodCallHandler { call, result in
      guard call.method == "sample", let args = call.arguments as? [String: Any],
        let path = args["path"] as? String
      else {
        result(FlutterMethodNotImplemented)
        return
      }
      let maxFrames = args["maxFrames"] as? Int ?? 24
      DispatchQueue.global(qos: .userInitiated).async {
        do {
          let frames = try harborSampleVideoFrames(path: path, maxFrames: maxFrames, maxEdge: 512)
          DispatchQueue.main.async { result(frames.map { FlutterStandardTypedData(bytes: $0) }) }
        } catch {
          DispatchQueue.main.async {
            result(FlutterError(code: "video", message: error.localizedDescription, details: nil))
          }
        }
      }
    }

    super.awakeFromNib()
  }
}

/// Sample up to `maxFrames` JPEG frames, evenly spaced across the video
/// (about 1 per second, the rate the embedding model expects).
func harborSampleVideoFrames(path: String, maxFrames: Int, maxEdge: CGFloat) throws -> [Data] {
  let asset = AVURLAsset(url: URL(fileURLWithPath: path))
  let duration = CMTimeGetSeconds(asset.duration)
  guard duration.isFinite, duration > 0 else {
    throw NSError(domain: "harbor.video", code: 1, userInfo: [NSLocalizedDescriptionKey: "unreadable video"])
  }
  let count = max(1, min(maxFrames, Int(ceil(duration))))
  let generator = AVAssetImageGenerator(asset: asset)
  generator.appliesPreferredTrackTransform = true
  // Exact seeking: with a loose tolerance a short clip with one keyframe
  // returns the SAME frame for every sample.
  generator.maximumSize = CGSize(width: maxEdge, height: maxEdge)
  generator.requestedTimeToleranceBefore = .zero
  generator.requestedTimeToleranceAfter = .zero
  var frames: [Data] = []
  for i in 0..<count {
    let t = CMTime(seconds: duration * (Double(i) + 0.5) / Double(count), preferredTimescale: 600)
    let image = try generator.copyCGImage(at: t, actualTime: nil)
    let data = NSMutableData()
    guard let dest = CGImageDestinationCreateWithData(data, UTType.jpeg.identifier as CFString, 1, nil) else {
      throw NSError(domain: "harbor.video", code: 2, userInfo: [NSLocalizedDescriptionKey: "jpeg encoder"])
    }
    CGImageDestinationAddImage(dest, image, [kCGImageDestinationLossyCompressionQuality: 0.8] as CFDictionary)
    guard CGImageDestinationFinalize(dest) else {
      throw NSError(domain: "harbor.video", code: 3, userInfo: [NSLocalizedDescriptionKey: "jpeg encode failed"])
    }
    frames.append(data as Data)
  }
  return frames
}
