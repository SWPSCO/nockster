import Foundation
import SwiftUI
import WebKit
import Capacitor

struct WalletAccount: Codable, Identifiable, Hashable {
    let id: String
    let name: String
    let address: String
    let balance: String?
    let balanceNicks: String?
    let balanceUsd: String?
}

struct TransactionParty: Codable, Identifiable {
    let address: String
    let label: String?
    var id: String { address }
}

struct BridgeSummary: Codable {
    let destination: String
    let amount: String
    let protocolFee: String
    let expectedReceived: String
}

struct BridgeProgress: Codable {
    let blocks: Int?
    let target: Int
    let phase: String
    let label: String
}

struct WalletTransaction: Codable, Identifiable {
    let bridgeProgress: BridgeProgress?
    let bridge: BridgeSummary?
    let id: String
    let amount: String
    let fee: String
    let amountUsd: String?
    let feeUsd: String?
    let timestamp: Double
    let status: String
    let direction: String
    let statusLabel: String
    let pendingDetail: String?
    let canClear: Bool
    let parties: [TransactionParty]
    let blockHeight: Int?
    let error: String?
    var date: Date { Date(timeIntervalSince1970: timestamp / 1000) }
}

struct AddressAlias: Codable, Identifiable {
    let id: String
    let alias: String
    let address: String
}

struct WalletSnapshot: Codable {
    var exists = false
    var unlocked = false
    var suggestedWalletName = "My Wallet"
    var activeId: String?
    var wallets: [WalletAccount] = []
    var history: [WalletTransaction] = []
    var contacts: [AddressAlias] = []
    var networkError: String?
    var usdPerNock: Double?
    var vanitySearch: VanitySession?
    var active: WalletAccount? { wallets.first { $0.id == activeId } }
}

struct PaymentRecipient: Codable, Hashable {
    var address: String
    var amount: String
    var amountNicks: String? = nil
    var amountUsd: String? = nil
}

struct PaymentPreview: Codable, Identifiable {
    let bridge: BridgeSummary?
    let netSent: String
    let netSentNicks: String
    let netSentUsd: String?
    let feeUsd: String?
    let totalUsd: String?
    let id: String
    let recipients: [PaymentRecipient]
    let fee: String
    let feeNicks: String
    let totalNicks: String
    let total: String
    let privateOutputs: Bool
}

struct VanityProgress: Codable {
    var status = "idle"
    var message = ""
    var attempts: Double = 0
    var rate: Double = 0
    var seconds: Double = 0
    var backend = ""
    var address: String?
}

struct VanityOptions: Codable {
    let prefix: String
    let insensitive: Bool
    let keyMode: String
    let backend: String
    let lanes: Int
    let maxAttempts: Double
}

struct VanitySession: Codable {
    let id: String
    let name: String
    let options: VanityOptions
    let progress: VanityProgress
}

struct WalletCandidate: Codable {
    let kind: String
    var key: String
    let address: String?
}

struct WalletReply: Codable {
    let state: WalletSnapshot?
    let error: String?
    let mnemonic: [String]?
    let vanity: VanityProgress?
    let candidate: WalletCandidate?
    let preview: PaymentPreview?
    let txId: String?
    let unlockKey: String?
    let amounts: [String]?
    let address: String?
}

struct WalletFailure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

final class WalletEngineController: CAPBridgeViewController {
    override func instanceDescriptor() -> InstanceDescriptor {
        let descriptor = super.instanceDescriptor()
        descriptor.appStartPath = "/index.html"
        return descriptor
    }
}

@MainActor
final class WalletModel: ObservableObject {
    @Published var snapshot = WalletSnapshot()
    @Published var deviceUnlockEnabled = DeviceUnlock.enabled
    private var authenticating = false
    private var backgrounded = false
    private var active = false
    private var automaticUnlockPending = true
    @Published var ready = false
    @Published var busy = false
    @Published var error: String?
    @Published var notice: String?
    @Published var feedbackSubject = ""
    @Published var feedbackMessage = ""
    @Published var privateScreen = false
    var cameraPermissionPrompt = false
    @Published var sessionID = UUID()
    @Published var pendingRecipients: [PaymentRecipient] = []
    let engine = WalletEngineController()
    private var started = false
    private var epoch = 0
    private var lockTask: Task<Void, Never>?

    func start() async {
        guard !started else { return }
        started = true
        engine.loadViewIfNeeded()
        for _ in 0..<100 {
            do {
                guard let webView = engine.webView else { throw WalletFailure(message: "Engine is loading") }
                let available = try await webView.evaluateJavaScript("Boolean(window.nocksterNative)") as? Bool
                if available == true {
                    let reply = try await call("status")
                    if let state = reply.state { snapshot = state }
                    ready = true
                    error = reply.error
                    await automaticallyUnlock()
                    return
                }
            } catch { /* The bundled engine is loading. */ }
            try? await Task.sleep(for: .milliseconds(200))
        }
        error = "The wallet engine could not start. Close and reopen Nockster."
    }

    func call(_ action: String, _ fields: [String: Any] = [:]) async throws -> WalletReply {
        guard let webView = engine.webView else { throw WalletFailure(message: "Wallet engine is unavailable") }
        var request = fields
        request["action"] = action
        let value = try await webView.callAsyncJavaScript(
            "return await window.nocksterNative.dispatch(request)",
            arguments: ["request": request], in: nil, contentWorld: .page
        )
        guard let json = value as? String, let data = json.data(using: .utf8) else {
            throw WalletFailure(message: "Invalid wallet response")
        }
        return try JSONDecoder().decode(WalletReply.self, from: data)
    }

    @discardableResult
    func stopVanity(_ id: String) async -> VanityProgress? {
        let requestEpoch = epoch
        guard let reply = try? await call("vanityStop", ["searchId": id]), requestEpoch == epoch else { return nil }
        if snapshot.vanitySearch?.id == id { snapshot.vanitySearch = reply.state?.vanitySearch }
        return reply.vanity
    }

    func updateVanityStatus() async {
        guard !busy && !privateScreen, let id = snapshot.vanitySearch?.id else { return }
        let requestEpoch = epoch
        guard let reply = try? await call("vanityStatus", ["searchId": id]),
              requestEpoch == epoch, snapshot.vanitySearch?.id == id else { return }
        snapshot.vanitySearch = reply.state?.vanitySearch
    }

    @discardableResult
    func perform(_ action: String, _ fields: [String: Any] = [:]) async -> WalletReply? {
        guard !busy && !privateScreen else { return nil }
        busy = true
        error = nil
        let requestEpoch = epoch
        defer { busy = false }
        do {
            let reply = try await call(action, fields)
            guard requestEpoch == epoch else { return nil }
            if let state = reply.state { snapshot = state }
            if let failure = reply.error { throw WalletFailure(message: failure) }
            return reply
        } catch {
            if requestEpoch == epoch { self.error = error.localizedDescription }
            return nil
        }
    }

    func lock() {
        automaticUnlockPending = false
        epoch += 1
        sessionID = UUID()
        snapshot.unlocked = false
        snapshot.wallets = []
        snapshot.history = []
        snapshot.contacts = []
        snapshot.activeId = nil
        error = nil
        lockTask = Task {
            do { _ = try await call("lock") }
            catch { self.error = "Unable to lock the wallet engine. Close Nockster before continuing." }
        }
    }

    func enableDeviceUnlock(password: String) async {
        guard !busy, !authenticating, !privateScreen, snapshot.unlocked else { return }
        guard let key = await perform("deviceUnlockKey", ["password": password])?.unlockKey else { return }
        authenticating = true
        busy = true
        defer { authenticating = false; busy = false }
        do {
            try await DeviceUnlock.enable(secret: key)
            deviceUnlockEnabled = true
        } catch { self.error = error.localizedDescription }
    }

    func unlockWithDevice() async {
        guard ready, deviceUnlockEnabled, snapshot.exists, !snapshot.unlocked, !busy, !authenticating, active else { return }
        automaticUnlockPending = false
        authenticating = true
        busy = true
        let requestEpoch = epoch
        defer { authenticating = false; busy = false }
        do {
            let key = try await DeviceUnlock.secret()
            guard !backgrounded, requestEpoch == epoch else { return }
            await lockTask?.value
            guard !backgrounded, requestEpoch == epoch else { return }
            privateScreen = false
            busy = false
            _ = await perform("unlockWithDeviceKey", ["key": key])
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func automaticallyUnlock() async {
        guard automaticUnlockPending, active, !backgrounded, ready, deviceUnlockEnabled,
              snapshot.exists, !snapshot.unlocked, !privateScreen, !busy, !authenticating else { return }
        await unlockWithDevice()
    }

    func enterBackground() {
        backgrounded = true
        privateScreen = true
        lock()
        automaticUnlockPending = true
    }

    func confirmSubmission(_ action: String, fields: [String: Any], password: String?) async -> WalletReply? {
        guard !busy, !privateScreen, snapshot.unlocked else { return nil }
        var fields = fields
        if let password {
            fields["password"] = password
            return await perform(action, fields)
        }
        let requestEpoch = epoch
        authenticating = true
        busy = true
        defer { authenticating = false; busy = false }
        do {
            let key = try await DeviceUnlock.secret(reason: "Confirm this Nockster payment")
            guard !backgrounded, requestEpoch == epoch, snapshot.unlocked else {
                throw WalletFailure(message: "Unlock the wallet and review the payment again")
            }
            fields["key"] = key
            busy = false
            return await perform(action, fields)
        } catch { self.error = error.localizedDescription; return nil }
    }

    func setActive(_ active: Bool) {
        self.active = active
        if active { backgrounded = false }
        if authenticating || cameraPermissionPrompt { return }

        if !active {
            privateScreen = true
            lock()
        } else {
            Task {
                await lockTask?.value
                guard self.active else { return }
                privateScreen = false
                await automaticallyUnlock()
            }
        }
    }

    func receive(_ url: URL) {
        guard ["nockster", "web+nockster"].contains(url.scheme ?? ""),
              ["send", "pay"].contains(url.host ?? ""),
              url.user == nil, url.password == nil, url.port == nil,
              url.path.isEmpty || url.path == "/",
              let parts = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return }
        let addresses = parts.queryItems?.filter { $0.name == "to" }.compactMap(\.value) ?? []
        let amounts = parts.queryItems?.filter { $0.name == "amount" }.compactMap(\.value) ?? []
        guard !addresses.isEmpty, addresses.count == amounts.count, addresses.count <= 16 else { return }
        pendingRecipients = zip(addresses, amounts).map { PaymentRecipient(address: $0.0, amount: $0.1) }
    }
}

struct WalletEngineHost: UIViewControllerRepresentable {
    let model: WalletModel
    func makeUIViewController(context: Context) -> WalletEngineController {
        model.engine.view.isUserInteractionEnabled = false
        model.engine.view.accessibilityElementsHidden = true
        return model.engine
    }
    func updateUIViewController(_ controller: WalletEngineController, context: Context) {}
}
