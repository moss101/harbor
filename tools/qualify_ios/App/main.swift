import UIKit
class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    func application(_ a: UIApplication, didFinishLaunchingWithOptions o: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        let w = UIWindow(frame: UIScreen.main.bounds)
        let l = UILabel(frame: w.bounds); l.textAlignment = .center; l.numberOfLines = 0; l.text = "Running formula qualification…"
        let vc = UIViewController(); vc.view.backgroundColor = .white; vc.view.addSubview(l); w.rootViewController = vc; w.makeKeyAndVisible(); window = w
        DispatchQueue.global().async {
            let p = qualify_run("59a9269")!
            let json = String(cString: p); qualify_free(p)
            let dir = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            try? json.write(to: dir.appendingPathComponent("qualify.json"), atomically: true, encoding: .utf8)
            DispatchQueue.main.async { l.text = "Qualification finished (\(json.count) bytes)" }
            print("QUALIFY_DONE \(json.count)")
        }
        return true
    }
}
UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(AppDelegate.self))
