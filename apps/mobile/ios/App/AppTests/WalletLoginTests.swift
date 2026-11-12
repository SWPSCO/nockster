import XCTest
import SwiftUI
import WebKit
@testable import App

@MainActor
final class WalletLoginTests: XCTestCase {
    func testWalletCreationAndPasswordLogin() async throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let host = try XCTUnwrap(scene.windows.first?.rootViewController as? UIHostingController<WalletRootView>)
        let model = host.rootView.model
        for _ in 0..<150 where !model.ready {
            try await Task.sleep(for: .milliseconds(200))
        }
        XCTAssertTrue(model.ready, model.error ?? "Engine did not become ready")
        let webView = try XCTUnwrap(model.engine.webView)
        // Exercise the bundled WASM and native Preferences without contacting a real account.
        _ = try await webView.callAsyncJavaScript(
            "await window.Capacitor.Plugins.Preferences.configure({ group }); return null;",
            arguments: ["group": "NocksterLoginTests-" + UUID().uuidString], in: nil, contentWorld: .page
        )
        _ = try await webView.evaluateJavaScript("window.originalFetch = window.fetch; window.networkRequests = 0; window.fetch = async () => { window.networkRequests++; throw new Error('Offline test'); }; null;")
        let generated = try await model.call("generate")
        XCTAssertNil(generated.error)
        let words = try XCTUnwrap(generated.mnemonic)
        XCTAssertEqual(words.count, 24)
        let password = "test-only-wallet-password"
        let created = try await model.call("import", ["name": "Test Wallet", "key": words.joined(separator: " "), "password": password])
        XCTAssertNil(created.error)
        XCTAssertEqual(created.state?.unlocked, true)
        XCTAssertEqual(created.state?.wallets.count, 1)
        let requests = try await webView.evaluateJavaScript("window.networkRequests") as? Int
        XCTAssertEqual(requests, 0,
                       "Creating a wallet must not wait for the login service")
        let address = created.state?.wallets.first?.address
        let locked = try await model.call("lock")
        XCTAssertEqual(locked.state?.unlocked, false)
        let wrong = try await model.call("unlock", ["password": "wrong password"])
        XCTAssertNotNil(wrong.error)
        XCTAssertEqual(wrong.state?.unlocked, false)
        let opened = try await model.call("unlock", ["password": password])
        XCTAssertNil(opened.error)
        XCTAssertEqual(opened.state?.unlocked, true)
        XCTAssertEqual(opened.state?.wallets.first?.address, address)
        _ = try await webView.evaluateJavaScript(#"""
        window.authCalls = [];
        window.testServiceDown = true;
        window.fetch = async (url, options) => {
          if (!String(url).startsWith('https://')) return window.originalFetch(url, options);
          const body = JSON.parse(options?.body || '{}');
          const path = new URL(url).pathname;
          const json = data => new Response(JSON.stringify(data), {headers: {'Content-Type': 'application/json'}});
          window.authCalls.push(path);
          if (path === '/auth/iris/challenge') {
            const issued_at = new Date().toISOString();
            return json({challenge_id:'test', nonce:'test', address:body.address, account_type:'v1', issued_at,
              expires_at: new Date(Date.now()+600000).toISOString(),
              message:`Nockblocks Iris authentication\nAddress: ${body.address}\nOrigin: ${body.origin}\nChallenge: test\nIssued At: ${issued_at.replace(/\.\d{3}Z$/, 'Z')}`});
          }
          if (path === '/auth/iris/token') return window.testServiceDown
            ? new Response('Bad gateway', {status:502}) : json({status:'signed_in'});
          if (path === '/auth/keys') return json({key:'ak_test_native', api_key:{id:'test',expires_at:body.expires_at}});
          if (body.method === 'getTip') return json({price:0.02,result:{height:1}});
          if (body.method === 'getNotes') return json({result:{nicks:0,notes:[]}});
          if (body.method === 'getBlockchainMetrics') return json({result:{blockHeight:1}});
          if (body.method === 'getTransactionsByAddress') return json({result:{transactions:[]}});
          if (body.method === 'getAddressBook') return json({result:{entries:[]}});
          throw new Error('Unexpected test request: '+path+' '+body.method);
        }; null;
        """#)
        let unavailable = try await model.call("refresh")
        XCTAssertNil(unavailable.error)
        XCTAssertEqual(unavailable.state?.unlocked, true)
        XCTAssertEqual(unavailable.state?.wallets.first?.address, address)
        XCTAssertTrue(unavailable.state?.networkError?.contains("temporarily unavailable") == true)
        _ = try await webView.evaluateJavaScript("window.testServiceDown = false; null;")
        let refreshed = try await model.call("refresh")
        XCTAssertNil(refreshed.error)
        XCTAssertNil(refreshed.state?.networkError)
        let calls = try await webView.evaluateJavaScript("window.authCalls") as? [String]
        XCTAssertTrue(calls?.contains("/auth/iris/token") == true)
        XCTAssertTrue(calls?.contains("/auth/keys") == true)

        // Opening the native account view while biometric enrollment is busy
        // must refresh once enrollment finishes, rather than skip it forever.
        model.busy = true
        model.snapshot = try XCTUnwrap(opened.state)
        try await Task.sleep(for: .milliseconds(300))
        model.busy = false
        for _ in 0..<150 where model.snapshot.active?.balance == nil {
            try await Task.sleep(for: .milliseconds(100))
        }
        XCTAssertEqual(model.snapshot.active?.balance, "0.00")
        XCTAssertNil(model.error)
        model.snapshot.unlocked = false
        _ = try await model.call("lock")
        _ = try await webView.callAsyncJavaScript("await window.Capacitor.Plugins.Preferences.clear(); return null;", arguments: [:], in: nil, contentWorld: .page)
    }
}
