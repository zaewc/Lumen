// macOS rendering fixture for check-native-frame.mjs, using the actual WKWebView.
import Cocoa
import WebKit

final class FrameProbe: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
    let window: NSWindow
    let web: WKWebView
    let output: String

    init(url: URL, width: Int, dark: Bool, output: String) {
        self.output = output
        let config = WKWebViewConfiguration()
        web = WKWebView(frame: NSRect(x: 0, y: 0, width: width, height: 800), configuration: config)
        window = NSWindow(contentRect: web.frame, styleMask: [.titled], backing: .buffered, defer: false)
        super.init()
        config.userContentController.add(self, name: "probe")
        config.userContentController.addUserScript(WKUserScript(source: """
          window.violations = [];
          addEventListener('securitypolicyviolation', e => window.violations.push(e.violatedDirective));
          window.replied = false;
          addEventListener('message', e => {
            if (e.source === document.getElementById('__tauri_isolation__')?.contentWindow && e.data === 'pong') window.replied = true;
          });
        """, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        web.navigationDelegate = self
        window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        window.title = "Lumen rendering fixture"
        window.contentView = web
        window.center()
        window.makeKeyAndOrderFront(nil)
        web.load(URLRequest(url: url))
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        // The pinned Tauri 2.12.3 injection, with a local IPC stand-in as src.
        webView.evaluateJavaScript("""
          let style = document.createElement('style');
          style.textContent = '#__tauri_isolation__ { display: none !important }';
          document.head.append(style);
          let frame = document.createElement('iframe');
          frame.id = '__tauri_isolation__';
          frame.sandbox.add('allow-scripts');
          frame.src = '/isolation.html';
          frame.onload = () => frame.contentWindow.postMessage('ping', '*');
          document.body.append(frame);
          scrollTo(0, document.documentElement.scrollHeight);
          setTimeout(() => window.webkit.messageHandlers.probe.postMessage(JSON.stringify({
            display: getComputedStyle(frame).display,
            width: frame.getBoundingClientRect().width,
            height: frame.getBoundingClientRect().height,
            documentHeight: document.documentElement.scrollHeight,
            viewportHeight: innerHeight,
            replied: window.replied,
            blocked: window.violations
          })), 1000);
        """, completionHandler: nil)
    }

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        print(message.body)
        web.takeSnapshot(with: WKSnapshotConfiguration()) { img, error in
            guard let data = img?.tiffRepresentation,
                  let bitmap = NSBitmapImageRep(data: data),
                  let png = bitmap.representation(using: .png, properties: [:]) else {
                fputs("Snapshot failed: \(String(describing: error))\n", stderr)
                exit(1)
            }
            do { try png.write(to: URL(fileURLWithPath: self.output)) }
            catch { fputs("\(error)\n", stderr); exit(1) }
            NSApplication.shared.terminate(nil)
        }
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
guard CommandLine.arguments.count == 5,
      let url = URL(string: CommandLine.arguments[1]),
      let width = Int(CommandLine.arguments[2]) else { exit(1) }
let probe = FrameProbe(url: url, width: width, dark: CommandLine.arguments[3] == "dark", output: CommandLine.arguments[4])
DispatchQueue.main.asyncAfter(deadline: .now() + 20) { fputs("WebView timeout\n", stderr); exit(1) }
app.run()
