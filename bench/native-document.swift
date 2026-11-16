import AppKit
import WebKit

// Native benchmark runner: isolated WKWebView, no persistent app/browser data.
let arguments = CommandLine.arguments
func option(_ key: String, _ fallback: String = "") -> String {
    guard let index = arguments.firstIndex(of: key), index + 1 < arguments.count else { return fallback }
    return arguments[index + 1]
}
let target = option("--url", "http://127.0.0.1:1438/bench/document.html")
let output = option("--output", "/tmp/sag-wkwebview-report.json")
let screenshot = option("--screenshot")
let timeout = Double(option("--timeout", "210")) ?? 210
let runs = Int(option("--runs", "1")) ?? 0
if !(1...100).contains(runs) { fputs("Use 1 to 100 runs.\n", stderr); exit(2) }
func loopbackURL(_ value: String) -> URL {
    guard let url = URL(string: value), url.scheme == "http",
          ["127.0.0.1", "localhost", "::1", "[::1]"].contains(url.host ?? "") else {
        fputs("Use the loopback HTTP document benchmark URL.\n", stderr)
        exit(2)
    }
    return url
}
let targetURL = loopbackURL(target)
if !timeout.isFinite || timeout <= 0 { fputs("Use a positive finite timeout.\n", stderr); exit(2) }
#if arch(arm64)
let architecture = "arm64"
#elseif arch(x86_64)
let architecture = "x86_64"
#else
let architecture = "unknown"
#endif

final class BenchmarkWindow: NSWindow {
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect { frameRect }
}

final class Runner: NSObject, NSApplicationDelegate, WKNavigationDelegate {
    var window: NSWindow!
    var webView: WKWebView!
    var started = false
    var finished = false
    var pollTimer: Timer?
    var timeoutTimer: Timer?
    let start = Date()
    var navigations: [[String: Any]] = []
    func applicationDidFinishLaunching(_ notification: Notification) {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.userContentController.addUserScript(WKUserScript(source: """
        window.__sagNativeErrors = [];
        window.addEventListener('error', event => window.__sagNativeErrors.push(String(event.message).slice(0, 256)));
        window.addEventListener('unhandledrejection', event => window.__sagNativeErrors.push(String(event.reason?.message || event.reason).slice(0, 256)));
        """, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        webView = WKWebView(frame: NSRect(x: 0, y: 0, width: 1280, height: 850), configuration: configuration)
        webView.navigationDelegate = self
        window = BenchmarkWindow(contentRect: webView.frame, styleMask: [.titled, .closable], backing: .buffered, defer: false, screen: NSScreen.screens.first)
        window.contentMinSize = NSSize(width: 1280, height: 850)
        window.contentMaxSize = NSSize(width: 1280, height: 850)
        window.title = "SAG document renderer acceptance"
        window.ignoresMouseEvents = true
        window.contentView = webView
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApplication.shared.activate(ignoringOtherApps: true)
        window.setContentSize(NSSize(width: 1280, height: 850))
        timeoutTimer = Timer.scheduledTimer(withTimeInterval: timeout, repeats: false) { [weak self] _ in self?.finish(error: "Native runner timeout") }
        pollTimer = Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { [weak self] _ in self?.poll() }
        webView.load(URLRequest(url: targetURL))
    }
    func navigationEvent(_ phase: String) { navigations.append(["phase": phase, "elapsedMs": Date().timeIntervalSince(start) * 1000]) }
    func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) { navigationEvent("start") }
    func webView(_ webView: WKWebView, didCommit navigation: WKNavigation!) { navigationEvent("commit") }
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) { navigationEvent("finish") }
    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) { navigationEvent("web-content-terminated"); finish(error: "WebContent process terminated") }
    func poll() {
        guard !finished else { return }
        if !started {
            webView.evaluateJavaScript("Boolean(window.__SAG_DOCUMENT_BENCHMARK__?.ready)") { [weak self] result, error in
                guard let self, !self.started, !self.finished else { return }
                if result as? Bool == true {
                    self.started = true
                    self.webView.evaluateJavaScript("""
                    window.__sagNativeDone = false;
                    window.__sagNativeError = null;
                    Promise.resolve().then(async () => {
                      window.__sagNativeRuns = [];
                      for (let index = 0; index < \(runs); index++) {
                        const result = await window.__SAG_DOCUMENT_BENCHMARK__.run();
                        window.__sagNativeRuns.push(result);
                        if (result.status !== 'complete') throw new Error('Document benchmark did not complete');
                      }
                      return window.__sagNativeRuns.at(-1);
                    })
                      .then(result => { window.__sagNativeResult = result; window.__sagNativeDone = true; })
                      .catch(error => { window.__sagNativeError = String(error?.stack || error); window.__sagNativeDone = true; });
                    true;
                    """) { _, error in if let error { self.finish(error: String(describing: error)) } }
                }
            }
        } else {
            webView.evaluateJavaScript("Boolean(window.__sagNativeDone)") { [weak self] result, _ in
                if result as? Bool == true { self?.finish(error: nil) }
            }
        }
    }
    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) { finish(error: String(describing: error)) }
    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) { finish(error: String(describing: error)) }
    func finish(error: String?) {
        guard !finished else { return }
        finished = true
        pollTimer?.invalidate()
        timeoutTimer?.invalidate()
        var completed = false
        // Even if the web-content process stalls, emit bounded, actionable native output.
        let fallback = Timer.scheduledTimer(withTimeInterval: 5, repeats: false) { [weak self] _ in
            guard let self, !completed else { return }
            completed = true
            self.save(report: nil, error: error ?? "WebContent report export did not respond", screenshot: false)
        }
        webView.evaluateJavaScript("""
        JSON.stringify({
          report: window.__SAG_DOCUMENT_BENCHMARK__?.report?.() || window.__sagNativeResult || null,
          runs: window.__sagNativeRuns || [],
          error: window.__sagNativeError || null,
          javascriptErrors: window.__sagNativeErrors || [],
          visibility: { state: document.visibilityState, hasFocus: document.hasFocus() },
          userAgent: navigator.userAgent,
          viewport: { width: innerWidth, height: innerHeight, devicePixelRatio },
          location: location.pathname
        })
        """) { [weak self] result, exportError in
            guard let self, !completed else { return }
            completed = true
            fallback.invalidate()
            self.save(report: result as? String, error: error ?? exportError.map(String.init(describing:)), screenshot: !screenshot.isEmpty)
        }
    }
    func save(report: String?, error: String?, screenshot capture: Bool) {
        var object: [String: Any] = [
            "renderer": "native macOS WKWebView",
            "nativeElapsedMs": Date().timeIntervalSince(start) * 1000,
            "os": ProcessInfo.processInfo.operatingSystemVersionString,
            "processArchitecture": architecture,
            "persistentWebsiteData": false
        ]
        object["navigations"] = navigations
        object["nativeViewSize"] = [webView.bounds.width, webView.bounds.height]
        object["nativeBackingScaleFactor"] = window.backingScaleFactor
        object["nativeWindowVisible"] = window.isVisible
        object["nativeWindowOcclusionVisible"] = window.occlusionState.contains(.visible)
        object["nativeAppActiveAtEnd"] = NSApplication.shared.isActive
        if let report, let data = report.data(using: .utf8), let value = try? JSONSerialization.jsonObject(with: data) { object["browser"] = value }
        if let error { object["runnerError"] = error }
        let browser = object["browser"] as? [String: Any]
        let benchmark = browser?["report"] as? [String: Any]
        let javascriptErrors = browser?["javascriptErrors"] as? [String]
        var exitCode: Int32 = error == nil && benchmark?["status"] as? String == "complete"
            && browser?["error"] as? String == nil && javascriptErrors?.isEmpty == true ? 0 : 1
        do {
            let data = try JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys])
            try data.write(to: URL(fileURLWithPath: output), options: .atomic)
            print(String(data: data, encoding: .utf8)!)
        } catch { fputs("Report write failed: \(error)\n", stderr); exitCode = 1 }
        guard capture else { exit(exitCode) }
        webView.takeSnapshot(with: nil) { image, error in
            if let image, let tiff = image.tiffRepresentation, let bitmap = NSBitmapImageRep(data: tiff), let png = bitmap.representation(using: .png, properties: [:]) {
                do { try png.write(to: URL(fileURLWithPath: screenshot), options: .atomic) }
                catch { fputs("Snapshot write failed: \(error)\n", stderr); exitCode = 1 }
            } else { fputs("Snapshot failed: \(String(describing: error))\n", stderr); exitCode = 1 }
            exit(exitCode)
        }
    }
}
let app = NSApplication.shared
let runner = Runner()
app.delegate = runner
app.setActivationPolicy(.regular)
app.run()
