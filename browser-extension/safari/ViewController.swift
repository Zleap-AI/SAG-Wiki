import Cocoa
import SafariServices
import WebKit
import os

class ViewController: NSViewController, WKNavigationDelegate, WKScriptMessageHandler {
    @IBOutlet var webView: WKWebView!
    private var pageReady = false
    private var checkID = 0
    private var timeout: DispatchWorkItem?
    private let extensionIdentifier = Bundle.main.bundleIdentifier! + ".Extension"
    private let logger = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "Safari setup")

    override func viewDidLoad() {
        super.viewDidLoad()
        webView.navigationDelegate = self
        webView.configuration.userContentController.add(self, name: "controller")
        NotificationCenter.default.addObserver(self, selector: #selector(refreshState), name: NSApplication.didBecomeActiveNotification, object: nil)
        webView.loadFileURL(Bundle.main.url(forResource: "Main", withExtension: "html")!, allowingReadAccessTo: Bundle.main.resourceURL!)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        pageReady = true
        refreshState()
    }

    @objc private func refreshState() {
        guard pageReady else { return }
        checkID += 1
        let requestID = checkID
        timeout?.cancel()
        showState(nil)
        let deadline = DispatchWorkItem { [weak self] in
            guard let self, self.checkID == requestID else { return }
            self.showState(nil, error: "timeout")
        }
        timeout = deadline
        DispatchQueue.main.asyncAfter(deadline: .now() + 8, execute: deadline)
        SFSafariExtensionManager.getStateOfSafariExtension(withIdentifier: extensionIdentifier) { [weak self] state, error in
            DispatchQueue.main.async {
                guard let self, self.checkID == requestID else { return }
                self.timeout?.cancel()
                if let error {
                    self.report(error)
                } else if let state {
                    self.showState(state.isEnabled)
                } else {
                    self.showState(nil, error: "timeout")
                }
            }
        }
    }

    private func report(_ error: Error) {
        let error = error as NSError
        logger.error("Safari extension request failed: \(error.domain, privacy: .public) (\(error.code))")
        showState(nil, error: error.domain == SFErrorDomain && error.code == 1 ? "not-found" : "\(error.domain) (\(error.code)): \(error.localizedDescription)")
    }

    private func showState(_ enabled: Bool?, error: String = "") {
        webView.callAsyncJavaScript("show(enabled, error)", arguments: ["enabled": enabled as Any? ?? NSNull(), "error": error], in: nil, in: .page) { [weak self] result in
            if case .failure(let error) = result {
                self?.logger.error("Safari setup display failed: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        switch message.body as? String {
        case "close":
            NSApplication.shared.terminate(nil)
        case "check-state":
            refreshState()
        case "open-preferences":
            SFSafariApplication.showPreferencesForExtension(withIdentifier: extensionIdentifier) { [weak self] error in
                DispatchQueue.main.async {
                    if let error { self?.report(error) }
                    else { NSApplication.shared.terminate(nil) }
                }
            }
        default:
            break
        }
    }
}
