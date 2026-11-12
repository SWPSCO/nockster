import XCTest
import SwiftUI
import WebKit
@testable import App

/// Opt-in captures of the production native UI. No secret is compiled into the app.
/// Run only on a disposable simulator with scripts/ci/ios-store-screenshots.py.
@MainActor
final class WalletScreenshotTests: XCTestCase {
    func testCaptureStoreScreenshots() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let seedPath = env["NOCKSTER_SCREENSHOT_SEED"],
              let outputPath = env["NOCKSTER_SCREENSHOT_OUTPUT"] else {
            throw XCTSkip("App Store captures are opt-in")
        }
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = try XCTUnwrap(scene.windows.first)
        let host = try XCTUnwrap(window.rootViewController as? UIHostingController<WalletRootView>)
        let model = host.rootView.model
        for _ in 0..<150 where !model.ready { try await Task.sleep(for: .milliseconds(200)) }
        guard model.ready else { throw WalletFailure(message: "Engine did not start") }
        let webView = try XCTUnwrap(model.engine.webView)
        _ = try await webView.callAsyncJavaScript(
            "await window.Capacitor.Plugins.Preferences.configure({ group }); return null;",
            arguments: ["group": "NocksterScreenshots-" + UUID().uuidString], in: nil, contentWorld: .page
        )
        // Read at runtime; never print the recovery material or include it in an assertion.
        let key = try String(contentsOfFile: seedPath, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines)
        let imported = await model.perform("import", ["name": "Everyday", "key": key, "password": UUID().uuidString + UUID().uuidString])
        guard imported?.error == nil, model.snapshot.unlocked else {
            throw WalletFailure(message: "Screenshot wallet import failed")
        }
        // Prevent the capture session from submitting, retrying, or exporting secrets.
        _ = try await webView.evaluateJavaScript("""
        window.screenshotDispatch = window.nocksterNative.dispatch;
        window.nocksterNative.dispatch = request => {
          if (!['status', 'refresh', 'contacts', 'convertAmounts', 'prepare', 'lock'].includes(request.action))
            throw new Error('Action disabled in screenshot session');
          return window.screenshotDispatch(request);
        }; null;
        """)
        for _ in 0..<600 where model.busy { try await Task.sleep(for: .milliseconds(100)) }
        _ = await model.perform("refresh")
        guard model.snapshot.networkError == nil, model.snapshot.active?.balance != nil else {
            throw WalletFailure(message: "Screenshot wallet data could not refresh")
        }
        let directory = URL(fileURLWithPath: outputPath)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        window.overrideUserInterfaceStyle = .light
        try await Task.sleep(for: .seconds(2))
        try await capture("01-wallet", in: directory)
        let tabs = try XCTUnwrap(findTabs(host))
        for (index, name) in [(2, "02-receive"), (3, "03-activity"), (1, "04-send")] {
            if index == 1, let address = model.snapshot.active?.address {
                model.pendingRecipients = [PaymentRecipient(address: address, amount: "25")]
            }
            tabs.selectedIndex = index
            if let selected = tabs.selectedViewController { tabs.delegate?.tabBarController?(tabs, didSelect: selected) }
            try await Task.sleep(for: .seconds(2))
            try await capture(name, in: directory)
        }
        tabs.selectedIndex = 0
        if let selected = tabs.selectedViewController { tabs.delegate?.tabBarController?(tabs, didSelect: selected) }
        window.overrideUserInterfaceStyle = .dark
        try await Task.sleep(for: .seconds(2))
        try await capture("05-wallet-dark", in: directory)
        _ = await model.perform("lock")
        _ = try await webView.callAsyncJavaScript("await window.Capacitor.Plugins.Preferences.clear(); return null;", arguments: [:], in: nil, contentWorld: .page)
    }

    private func findTabs(_ controller: UIViewController) -> UITabBarController? {
        if let tabs = controller as? UITabBarController { return tabs }
        for child in controller.children { if let tabs = findTabs(child) { return tabs } }
        return nil
    }

    private func capture(_ name: String, in directory: URL) async throws {
        // Hosted unit tests cannot use XCUIScreen. Let simctl capture the whole
        // display (including the status bar) after the native view has settled.
        let request = directory.appendingPathComponent("capture-request.txt")
        try name.write(to: request, atomically: true, encoding: .utf8)
        for _ in 0..<300 {
            if !FileManager.default.fileExists(atPath: request.path) { return }
            try await Task.sleep(for: .milliseconds(100))
        }
        throw WalletFailure(message: "Screenshot exporter did not acknowledge capture")
    }
}
