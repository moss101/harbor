import Flutter
import UIKit

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    OfficeOpenIntake.register(with: engineBridge.pluginRegistry)
    OfficePrint.register(with: engineBridge.pluginRegistry)
  }
}

/// System print dialog over a rendered PDF (the office Print action).
/// The PDF bytes are handed to UIPrintInteractionController; nothing is
/// sent anywhere except the user's chosen printer.
final class OfficePrint: NSObject {
  static func register(with registry: FlutterPluginRegistry) {
    let channel = FlutterMethodChannel(
      name: "dev.harbor.office/print",
      binaryMessenger: registry.registrar(forPlugin: "HarborOfficePrint")!.messenger())
    channel.setMethodCallHandler { call, result in
      OfficePrint.handle(call: call, result: result)
    }
  }

  fileprivate static func handle(call: FlutterMethodCall, result: @escaping FlutterResult) {
    switch call.method {
    case "printPdf":
      guard let args = call.arguments as? [String: Any],
            let data = args["data"] as? FlutterStandardTypedData,
            let jobName = args["name"] as? String else {
        result(FlutterError(code: "print", message: "missing data/name", details: nil))
        return
      }
      let controller = UIPrintInteractionController.shared
      let info = UIPrintInfo(dictionary: nil)
      info.outputType = .general
      info.jobName = jobName
      controller.printInfo = info
      controller.printingItem = data.data
      controller.present(animated: true) { (_, completed, error) in
        if let error = error {
          result(FlutterError(code: "print", message: error.localizedDescription, details: nil))
        } else {
          result(completed)
        }
      }
    default:
      result(FlutterMethodNotImplemented)
    }
  }
}

/// "Open in Harbor Office Suite": documents other apps hand us (Files,
/// Mail, the share sheet's "Open in…").
///
/// The URL is security-scoped (the suite declares
/// LSSupportsOpeningDocumentsInPlace), so the grant is held only while the
/// bytes are COPIED into a per-delivery temp folder; Dart imports that
/// copy into the suite's own Documents/Opened store and deletes it.
/// `getInitialPath` returns the launch document once; later deliveries are
/// pushed as `openPath`. Payloads are `{path, name}` or `{error, name}`.
final class OfficeOpenIntake: NSObject, FlutterSceneLifeCycleDelegate {
  private static let maxBytes: Int64 = 256 * 1024 * 1024
  private var channel: FlutterMethodChannel?
  private var launchPayload: [String: String]?
  private var dartListening = false

  static func register(with registry: FlutterPluginRegistry) {
    guard let registrar = registry.registrar(forPlugin: "HarborOfficeOpenIntake") else { return }
    let intake = OfficeOpenIntake()
    let channel = FlutterMethodChannel(
      name: "dev.harbor.office/open", binaryMessenger: registrar.messenger())
    intake.channel = channel
    channel.setMethodCallHandler { call, result in
      switch call.method {
      case "getInitialPath":
        intake.dartListening = true
        let payload = intake.launchPayload
        intake.launchPayload = nil  // consumed once
        result(payload)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
    registrar.addSceneDelegate(intake)
  }

  // MARK: scene lifecycle

  func scene(
    _ scene: UIScene, willConnectTo session: UISceneSession,
    options connectionOptions: UIScene.ConnectionOptions?
  ) -> Bool {
    guard let contexts = connectionOptions?.urlContexts, !contexts.isEmpty else { return false }
    deliver(contexts, launch: true)
    return true
  }

  func scene(_ scene: UIScene, openURLContexts URLContexts: Set<UIOpenURLContext>) -> Bool {
    deliver(URLContexts, launch: !dartListening)
    return true
  }

  // MARK: delivery

  private func deliver(_ contexts: Set<UIOpenURLContext>, launch: Bool) {
    // One document at a time: the last URL wins (Work shows one file).
    guard let url = contexts.first(where: { $0.url.isFileURL })?.url else { return }
    DispatchQueue.global(qos: .userInitiated).async { [weak self] in
      let payload = Self.copyToTemp(url)
      DispatchQueue.main.async {
        guard let self = self else { return }
        if launch && !self.dartListening {
          self.launchPayload = payload
        } else {
          self.channel?.invokeMethod("openPath", arguments: payload)
        }
      }
    }
  }

  private static func copyToTemp(_ url: URL) -> [String: String] {
    let name = url.lastPathComponent
    let scoped = url.startAccessingSecurityScopedResource()
    defer { if scoped { url.stopAccessingSecurityScopedResource() } }
    let fm = FileManager.default
    do {
      let size = (try url.resourceValues(forKeys: [.fileSizeKey]).fileSize).map(Int64.init) ?? 0
      if size > maxBytes { return ["error": "too_large", "name": name] }
      let dir = fm.temporaryDirectory
        .appendingPathComponent("open-intake", isDirectory: true)
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
      try fm.createDirectory(at: dir, withIntermediateDirectories: true)
      let out = dir.appendingPathComponent(name)
      try fm.copyItem(at: url, to: out)
      // iOS stores a copy of every delivered document in the app's own
      // Documents/Inbox; once it is copied (and Dart imports it into
      // Documents/Opened) the Inbox copy is dead weight. Only ever delete
      // inside OUR Inbox — never a file opened in place from elsewhere.
      if let docs = fm.urls(for: .documentDirectory, in: .userDomainMask).first,
        url.standardizedFileURL.path.hasPrefix(
          docs.appendingPathComponent("Inbox").standardizedFileURL.path + "/")
      {
        try? fm.removeItem(at: url)
      }
      return ["path": out.path, "name": name]
    } catch {
      return ["error": "unreadable", "name": name]
    }
  }
}
