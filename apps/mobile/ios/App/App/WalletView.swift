import SwiftUI
import AVFoundation
import CoreImage.CIFilterBuiltins
import UIKit

struct WalletRootView: View {
    @ObservedObject var model: WalletModel
    @State private var tab = 0

    var body: some View {
        ZStack {
            WalletEngineHost(model: model)
                .frame(width: 1, height: 1)
                .opacity(0.001)
                .accessibilityHidden(true)
            Group {
                if !model.ready {
                    ProgressView("Opening Nockster…")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if !model.snapshot.exists || (model.snapshot.unlocked && model.snapshot.wallets.isEmpty) {
                    // Entry forms retain their in-memory drafts under the privacy cover.
                    NavigationStack { SetupView(model: model) }
                } else if !model.snapshot.unlocked {
                    UnlockView(model: model)
                } else {
                    TabView(selection: $tab) {
                        Tab("Wallet", systemImage: "wallet.bifold", value: 0) {
                            NavigationStack { AccountView(model: model) }
                        }
                        Tab("Send", systemImage: "arrow.up.right", value: 1) {
                            NavigationStack { SendView(model: model) }
                        }
                        Tab("Receive", systemImage: "qrcode", value: 2) {
                            NavigationStack { ReceiveView(model: model) }
                        }
                        Tab("Activity", systemImage: "clock", value: 3) {
                            NavigationStack { ActivityView(model: model) }
                        }
                        Tab("Settings", systemImage: "gearshape", value: 4) {
                            NavigationStack { WalletSettingsView(model: model) }
                        }
                    }
                    .id(model.sessionID)
                }
            }
            .background(NocksterPalette.background)
            if model.privateScreen {
                NocksterPalette.background.ignoresSafeArea()
                Image(systemName: "lock.shield").font(.system(size: 52)).foregroundStyle(.secondary)
            }
        }
        .tint(.primary)
        .task { await model.start() }
        .onChange(of: model.snapshot.unlocked) { _, unlocked in
            if unlocked { tab = model.pendingRecipients.isEmpty ? 0 : 1 }
        }
        .onChange(of: model.pendingRecipients) { _, recipients in
            if !recipients.isEmpty && model.snapshot.unlocked { tab = 1 }
        }
        .alert("Nockster", isPresented: Binding(get: { model.error != nil || model.notice != nil }, set: { if !$0 { model.error = nil; model.notice = nil } })) {
            Button("OK", role: .cancel) { model.error = nil; model.notice = nil }
        } message: { Text(model.error ?? model.notice ?? "") }
    }
}

struct UnlockView: View {
    @ObservedObject var model: WalletModel
    @State private var password = ""
    var body: some View {
        NavigationStack {
            VStack(spacing: 24) {
                Spacer()
                HStack(spacing: 10) {
                    Image("NocksterMark").renderingMode(.template).resizable().scaledToFit().frame(width: 48, height: 48)
                    Text("Nockster").font(.custom("Nokora-ExtraBold", size: 32, relativeTo: .largeTitle)).tracking(-0.64)
                }
                Text("Nockchain wallet").foregroundStyle(.secondary)
                SecureField("Wallet password", text: $password)
                    .textContentType(.password).textFieldStyle(.roundedBorder)
                    .submitLabel(.go).onSubmit { unlock() }
                    .accessibilityIdentifier("unlock-password")
                Button(action: unlock) {
                    if model.busy { ProgressView() }
                    else { Text("Unlock Wallet").frame(maxWidth: .infinity) }
                }
                .buttonStyle(.glassProminent).controlSize(.large)
                .disabled(password.isEmpty || model.busy)
                .accessibilityIdentifier("unlock-wallet")
                if model.deviceUnlockEnabled {
                    Button("Use Passcode or Biometrics", systemImage: "faceid") {
                        Task { await model.unlockWithDevice() }
                    }.buttonStyle(.glass).controlSize(.large).disabled(model.busy)
                }
                Spacer()
            }
            .padding(28).frame(maxWidth: 480)
            .frame(maxWidth: .infinity)
            .navigationTitle("")
        }
    }
    private func unlock() {
        let value = password
        password = ""
        Task { _ = await model.perform("unlock", ["password": value]) }
    }
}

struct SetupView: View {
    @ObservedObject var model: WalletModel
    @Environment(\.dismiss) private var dismiss
    @State private var importing = false
    @State private var name: String
    @State private var key = ""
    @State private var importKind = "mnemonic"
    @State private var customAddress = false
    @State private var minedKey = ""
    @State private var minedAddress = ""
    @State private var password = ""
    @State private var confirmation = ""
    @State private var enableDeviceUnlock = false
    @State private var mnemonic: [String] = []
    @State private var wordOne = ""
    @State private var wordLast = ""
    @State private var saved = false

    init(model: WalletModel) {
        self.model = model
        _name = State(initialValue: model.snapshot.suggestedWalletName)
    }

    private var confirmed: Bool {
        if !minedKey.isEmpty { return saved }
        return !mnemonic.isEmpty && saved && wordOne.trimmingCharacters(in: .whitespacesAndNewlines) == mnemonic.first &&
        wordLast.trimmingCharacters(in: .whitespacesAndNewlines) == mnemonic.last
    }
    private var nameExists: Bool {
        model.snapshot.wallets.contains { $0.name == name.trimmingCharacters(in: .whitespacesAndNewlines) }
    }
    private var valid: Bool {
        !nameExists &&
        !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
        (importing ? !key.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty : confirmed) &&
        (model.snapshot.exists || (password.count >= 12 && password == confirmation))
    }

    var body: some View {
        Form {
            Section {
                VStack(alignment: .leading, spacing: 10) {
                    Image(systemName: "wallet.bifold.fill").font(.largeTitle)
                    Text("A Nockchain mobile wallet just for you.").font(.title.bold())
                    Text("Create a Nockchain wallet or bring your existing keys.").foregroundStyle(.secondary)
                }.padding(.vertical, 16)
                Picker("Wallet", selection: $importing) {
                    Text("Create").tag(false)
                    Text("Import").tag(true)
                }.pickerStyle(.segmented)
                TextField("Wallet name", text: $name).accessibilityIdentifier("wallet-name")
                    .foregroundStyle(nameExists ? Color.red : Color.primary)
                if nameExists { Text("A wallet with this name already exists").font(.caption).foregroundStyle(.red) }
            }
            if importing {
                Section {
                    Picker("Import with", selection: $importKind) {
                        Text("24-word seed phrase").tag("mnemonic")
                        Text("Secret key · hex").tag("raw")
                        Text("Extended private key").tag("extended")
                    }
                    TextEditor(text: $key).frame(minHeight: importKind == "mnemonic" ? 120 : 80)
                        .font(importKind == "raw" ? .system(.body, design: .monospaced) : .body)
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                        .accessibilityLabel(importKind == "raw" ? "Secret key in hex" : "Recovery material")
                        .accessibilityIdentifier("import-key")
                } header: { Text(importKind == "raw" ? "Secret key" : "Recovery material") }
                  footer: { Text(importKind == "raw" ? "Enter 64 hexadecimal characters, with an optional 0x prefix. A raw key has no seed phrase." : importKind == "extended" ? "Paste your zprv extended private key." : "Enter your 24 words separated by spaces.") }
            } else if mnemonic.isEmpty && minedKey.isEmpty {
                Section {
                    Toggle("Custom address", isOn: $customAddress)
                } footer: { Text("Choose how your address starts. Search runs on this device.") }
                if customAddress {
                    VanityControls(model: model) { candidate in
                        minedAddress = candidate.address ?? ""
                        if candidate.kind == "mnemonic" { mnemonic = candidate.key.split(separator: " ").map(String.init) }
                        else { minedKey = candidate.key }
                        saved = false
                    }
                } else {
                    Section {
                        Button("Generate Recovery Phrase", systemImage: "sparkles") {
                            Task {
                                if let reply = await model.perform("generate") { mnemonic = reply.mnemonic ?? [] }
                            }
                        }.disabled(model.busy)
                    }
                }
            } else if !minedKey.isEmpty {
                Section {
                    Text(minedKey).font(.system(.body, design: .monospaced)).textSelection(.enabled)
                        .accessibilityIdentifier("generated-secret-key")
                    Toggle("I saved my secret key somewhere private", isOn: $saved)
                } header: { Text("Back up your secret key") }
                  footer: { Text("This wallet has no seed phrase. Save this key privately to restore it.") }
                if !minedAddress.isEmpty {
                    Section("Wallet address") { Text(minedAddress).font(.system(.caption, design: .monospaced)).textSelection(.enabled) }
                }
            } else {
                Section {
                    LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], alignment: .leading, spacing: 12) {
                        ForEach(Array(mnemonic.enumerated()), id: \.offset) { index, word in
                            HStack {
                                Text("\(index + 1)").foregroundStyle(.secondary).frame(width: 24, alignment: .trailing)
                                Text(word).fontWeight(.medium)
                            }.font(.system(.body, design: .monospaced))
                        }
                    }.padding(.vertical, 8)
                    Toggle("I wrote down all 24 words", isOn: $saved)
                    TextField("First word", text: $wordOne)
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                    TextField("Last word", text: $wordLast)
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                } header: { Text("Secret recovery phrase") }
                  footer: { Text("Keep these words private and offline. Anyone with them can spend your funds.") }
            }
            if !model.snapshot.exists {
                Section {
                    SecureField("Wallet password", text: $password).textContentType(.newPassword)
                        .accessibilityIdentifier("setup-password")
                    Label(password.count >= 12 ? "Minimum length met" : "\(password.count)/12 characters",
                          systemImage: password.count >= 12 ? "checkmark.circle" : "info.circle")
                        .font(.footnote).foregroundStyle(.secondary)
                    SecureField("Confirm password", text: $confirmation).textContentType(.newPassword)
                        .accessibilityIdentifier("setup-confirm-password")
                    if !confirmation.isEmpty {
                        Label(password == confirmation ? "Passwords match" : "Passwords don’t match",
                              systemImage: password == confirmation ? "checkmark.circle" : "exclamationmark.circle")
                            .font(.footnote)
                            .foregroundStyle(password == confirmation ? Color.secondary : Color.red)
                    }
                } header: { Text("Protect this device") }
                  footer: {
                      Text("Use at least 12 characters. Numbers, symbols, and uppercase letters are optional. Try a few unrelated words.\n\nNockster locks when you leave the app. This password encrypts your wallet on this device.")
                  }
                Section {
                    Toggle("Enable device passcode or biometrics", isOn: $enableDeviceUnlock)
                        .disabled(model.busy)
                } footer: {
                    Text("Use this device’s passcode, Face ID, or Touch ID to unlock. This is optional and can also be enabled in Settings. Your wallet password still works.")
                }
            }
            Section {
                Button {
                    let value = importing ? key : minedKey.isEmpty ? mnemonic.joined(separator: " ") : minedKey
                    let setupPassword = password
                    let enrollDeviceUnlock = !model.snapshot.exists && enableDeviceUnlock
                    Task {
                        if await model.perform("import", ["name": name, "key": value, "password": setupPassword]) != nil {
                            key = ""; password = ""; confirmation = ""; mnemonic = []; minedKey = ""
                            if enrollDeviceUnlock { await model.enableDeviceUnlock(password: setupPassword) }
                            dismiss()
                        }
                    }
                } label: {
                    HStack {
                        Text(importing ? "Import Wallet" : "Create Wallet")
                        Spacer()
                        if model.busy { ProgressView() } else { Image(systemName: "arrow.right") }
                    }
                }.disabled(!valid || model.busy)
                .accessibilityIdentifier("save-wallet")
            }
        }
        .navigationTitle(model.snapshot.wallets.isEmpty ? "Welcome to Nockster" : "Add a wallet")
        .onChange(of: importKind) { _, _ in key = "" }
        .onChange(of: importing) { _, _ in key = ""; mnemonic = []; minedKey = ""; minedAddress = ""; saved = false; wordOne = ""; wordLast = "" }
        .onDisappear { key = ""; password = ""; confirmation = ""; mnemonic = []; minedKey = "" }
    }
}

private struct VanityControls: View {
    @ObservedObject var model: WalletModel
    let onFound: (WalletCandidate) -> Void
    @State private var prefix = ""
    @State private var insensitive = false
    @State private var recovery = "mnemonic"
    @State private var backend = "auto"
    @State private var lanes = 4096
    @State private var steps = 1
    @State private var limit = "0"
    @State private var advanced = false
    @State private var progress = VanityProgress()
    @State private var searchID = UUID().uuidString
    @State private var active = true
    private var mining: Bool { progress.status == "mining" }

    var body: some View {
        Section {
            TextField("Address starts with, e.g. nock", text: $prefix)
                .textInputAutocapitalization(.never).autocorrectionDisabled()
                .accessibilityIdentifier("vanity-prefix")
            Toggle("Ignore case and match letter / digit equivalents", isOn: $insensitive)
            if insensitive { Text("a / 4 · b / 8 · e / 3 · i / 1 · l / 1 · o / 0 · s / 5 · t / 7 · z / 2. i and l stay distinct.").font(.caption).foregroundStyle(.secondary) }
            Picker("Recovery", selection: $recovery) {
                Text("24-word seed phrase").tag("mnemonic")
                Text("Secret key (no phrase)").tag("raw")
            }
            Text("Seed phrases take longer to find. A secret key has no recovery phrase.").font(.caption).foregroundStyle(.secondary)
            DisclosureGroup("Search settings", isExpanded: $advanced) {
                Picker("Compute with", selection: $backend) {
                    Text("Automatic · GPU or CPU").tag("auto")
                    Text("CPU only").tag("cpu")
                }
                HStack { Text("GPU lanes"); TextField("GPU lanes", value: $lanes, format: .number).keyboardType(.numberPad).multilineTextAlignment(.trailing) }
                if recovery == "raw" { Stepper("Steps per batch: \(steps)", value: $steps, in: 1...16) }
                HStack { Text("Attempt limit"); TextField("Attempt limit", text: $limit).keyboardType(.numberPad).multilineTextAlignment(.trailing) }
                Text("0 means no limit. Shorter prefixes are faster to find.").font(.caption).foregroundStyle(.secondary)
            }
        }.disabled(mining || model.busy)
        Section {
            if !progress.message.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Text(progress.message)
                    if progress.attempts > 0 {
                        Text("\(Int(progress.attempts).formatted()) tried · \(Int(progress.rate).formatted())/s · \(progress.backend)").font(.caption).foregroundStyle(.secondary)
                    }
                    if let address = progress.address { Text(address).font(.system(.caption, design: .monospaced)).textSelection(.enabled) }
                }
            }
            if mining {
                Button("Stop Search") { Task {
                    if let reply = await model.perform("vanityStop", ["searchId": searchID]) { progress = reply.vanity ?? VanityProgress() }
                } }
            } else if progress.status == "found" {
                Button("Use This Address") { Task {
                    if let reply = await model.perform("vanityTake", ["searchId": searchID]), let candidate = reply.candidate { onFound(candidate) }
                } }.disabled(model.busy)
            } else {
                Button("Find Address", systemImage: "sparkles") { start() }
                    .disabled(prefix.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.busy)
            }
        } footer: { Text("Your keys stay on this device.") }
        .onChange(of: recovery) { _, value in lanes = value == "raw" ? 64 : 4096; resetResult() }
        .onChange(of: prefix) { _, _ in resetResult() }
        .onChange(of: insensitive) { _, _ in resetResult() }
        .onChange(of: backend) { _, _ in resetResult() }
        .onChange(of: lanes) { _, _ in resetResult() }
        .onChange(of: steps) { _, _ in resetResult() }
        .onChange(of: limit) { _, _ in resetResult() }
        .onChange(of: model.privateScreen) { _, hidden in if hidden { progress = VanityProgress(status: "stopped", message: "Search stopped when the app locked.") } }
        .task(id: mining) {
            while mining && !Task.isCancelled {
                do {
                    try await Task.sleep(for: .milliseconds(500))
                    if model.privateScreen || Task.isCancelled { break }
                    let reply = try await model.call("vanityStatus", ["searchId": searchID])
                    if Task.isCancelled { break }
                    if let value = reply.vanity { progress = value }
                    if let error = reply.error { progress = VanityProgress(status: "error", message: error) }
                } catch is CancellationError { break }
                catch { progress = VanityProgress(status: "error", message: error.localizedDescription) }
            }
        }
        .onAppear { active = true }
        .onDisappear {
            active = false
            let id = searchID
            Task { _ = try? await model.call("vanityStop", ["searchId": id]) }
        }
    }

    private func resetResult() {
        guard !mining else { return }
        progress = VanityProgress()
        let id = searchID
        Task { _ = try? await model.call("vanityStop", ["searchId": id]) }
        searchID = UUID().uuidString
    }

    private func start() {
        guard let attempts = Double(limit), attempts.isFinite, attempts >= 0, attempts.rounded() == attempts, attempts <= 9007199254740991 else {
            progress = VanityProgress(status: "error", message: "Enter a nonnegative whole number for the attempt limit."); return
        }
        let id = searchID
        let settings: [String: Any] = ["prefix": prefix, "insensitive": insensitive, "keyMode": recovery, "backend": backend, "lanes": lanes, "steps": steps, "maxAttempts": attempts]
        Task {
            guard active else { return }
            if let reply = await model.perform("vanityStart", ["searchId": id, "vanity": settings]) {
                if active && searchID == id { progress = reply.vanity ?? VanityProgress() }
                else { _ = try? await model.call("vanityStop", ["searchId": id]) }
            }
        }
    }
}

private func shortWalletAddress(_ address: String) -> String {
    address.count > 12 ? "\(address.prefix(6))…\(address.suffix(4))" : address
}

private func walletExplorerURL(_ address: String) -> URL? {
    guard !address.isEmpty else { return nil }
    return URL(string: "https://nockblocks.com/address/\(address)")
}

private func groupedAmount(_ value: String) -> String {
    let parts = value.split(separator: ".", maxSplits: 1, omittingEmptySubsequences: false)
    guard let whole = parts.first else { return value }
    let grouped = whole.replacingOccurrences(of: #"(?<=\d)(?=(\d{3})+$)"#, with: ",", options: .regularExpression)
    return grouped + (parts.count > 1 ? "." + parts[1] : "")
}

private struct WalletAddressActions: View {
    let address: String
    @State private var copied = false
    var body: some View {
        HStack(spacing: 18) {
            Button(copied ? "Copied" : "Copy address", systemImage: "doc.on.doc") {
                UIPasteboard.general.string = address
                copied = true
            }.buttonStyle(.borderless)
            if let url = walletExplorerURL(address) {
                Link(destination: url) { Label("Nockblocks", systemImage: "arrow.up.right.square") }
            }
        }.font(.caption).tint(.secondary)
        .onChange(of: address) { copied = false }
    }
}

private struct ManageWalletView: View {
    @ObservedObject var model: WalletModel
    let wallet: WalletAccount
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var backupConfirmed = false
    @State private var deleting = false
    @State private var confirmation = ""
    private var nameExists: Bool {
        model.snapshot.wallets.contains { $0.id != wallet.id && $0.name == name.trimmingCharacters(in: .whitespacesAndNewlines) }
    }
    var body: some View {
        Form {
            Section("Wallet name") {
                TextField("Wallet name", text: $name).foregroundStyle(nameExists ? Color.red : Color.primary)
                if nameExists { Text("A wallet with this name already exists").font(.caption).foregroundStyle(.red) }
                Button("Save name") {
                    Task {
                        if await model.perform("rename", ["walletId": wallet.id, "name": name]) != nil { dismiss() }
                    }
                }.disabled(nameExists || name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.busy)
            }
            Section {
                Text(wallet.address).font(.caption.monospaced()).textSelection(.enabled)
                WalletAddressActions(address: wallet.address)
            }
            Section {
                if model.snapshot.wallets.count > 1 {
                    Text("Deleting removes this wallet from this device. You need its recovery phrase or extended private key to restore access to its funds.")
                    Toggle("I have backed up this wallet’s recovery phrase or extended private key", isOn: $backupConfirmed)
                    Button("Delete Wallet", role: .destructive) { confirmation = ""; deleting = true }
                        .disabled(!backupConfirmed || model.busy)
                } else {
                    Text("Add another wallet before deleting your only wallet.").foregroundStyle(.secondary)
                }
            } header: { Text("Delete wallet") }
        }
        .navigationTitle("Manage Wallet").navigationBarTitleDisplayMode(.inline)
        .onAppear { name = wallet.name }
        .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.disabled(model.busy) } }
        .interactiveDismissDisabled(model.busy)
        .alert("Delete \(wallet.name)?", isPresented: $deleting) {
            TextField("Type wallet name", text: $confirmation).textInputAutocapitalization(.never).autocorrectionDisabled()
            Button("Cancel", role: .cancel) {}
            Button("Delete Wallet", role: .destructive) {
                Task {
                    if await model.perform("deleteWallet", ["walletId": wallet.id, "confirmation": confirmation, "backupConfirmed": backupConfirmed]) != nil { dismiss() }
                }
            }.disabled(confirmation != wallet.name || model.busy)
        } message: { Text("Type “\(wallet.name)” to confirm deletion from this device. This cannot be undone without your recovery backup.") }
    }
}

struct AccountView: View {
    @ObservedObject var model: WalletModel
    @State private var addingWallet = false
    @State private var editingWallet: WalletAccount?
    @State private var showNicks = false
    @Environment(\.colorScheme) private var colorScheme
    private var balanceColor: Color {
        showNicks ? (colorScheme == .dark ? Color(red: 0.37, green: 0.92, blue: 0.83) : Color(red: 0.06, green: 0.46, blue: 0.43)) : .primary
    }
    var body: some View {
        List {
            Section {
                VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        Text(model.snapshot.active?.name ?? "Wallet").font(.headline)
                        if let address = model.snapshot.active?.address { Text(shortWalletAddress(address)).font(.caption.monospaced()).foregroundStyle(.secondary) }
                    }
                    Text("Available balance").foregroundStyle(.secondary)
                    Button { showNicks.toggle() } label: {
                        VStack(alignment: .leading, spacing: 10) {
                            Text((showNicks ? model.snapshot.active?.balanceNicks : model.snapshot.active?.balance) ?? "—")
                                .font(.system(.largeTitle, design: .default, weight: .semibold))
                                .minimumScaleFactor(0.5).lineLimit(1)
                            Label(showNicks ? "NICKS" : "NOCK", systemImage: "arrow.left.arrow.right").font(.headline)
                        }.foregroundStyle(balanceColor)
                    }.buttonStyle(.plain).disabled(model.snapshot.active?.balance == nil)
                        .accessibilityHint(showNicks ? "Show balance in NOCK" : "Show balance in nicks")
                        .onChange(of: model.snapshot.activeId) { showNicks = false }
                    UsdSubtitle(value: model.snapshot.active?.balanceUsd)
                }.padding(.vertical, 24)
            }
            if let failure = model.snapshot.networkError {
                Section {
                    Label("Some information could not update", systemImage: "wifi.exclamationmark")
                    Text(failure).font(.footnote).foregroundStyle(.secondary)
                }
            }
            Section("Your wallets") {
                ForEach(model.snapshot.wallets) { wallet in
                    VStack(alignment: .leading, spacing: 12) {
                        HStack {
                            Button {
                                Task {
                                    if await model.perform("select", ["walletId": wallet.id]) != nil {
                                        _ = await model.perform("refresh")
                                    }
                                }
                            } label: {
                                VStack(alignment: .leading, spacing: 6) {
                                    HStack {
                                        Text(wallet.name).foregroundStyle(.primary).lineLimit(1)
                                        Text(shortWalletAddress(wallet.address)).font(.caption.monospaced()).foregroundStyle(.secondary)
                                    }
                                    Text(wallet.balance.map { "\($0) NOCK" } ?? "Balance unavailable")
                                        .font(.subheadline).foregroundStyle(.secondary)
                                }.frame(maxWidth: .infinity, alignment: .leading)
                            }.buttonStyle(.borderless).disabled(model.busy)
                            if wallet.id == model.snapshot.activeId { Image(systemName: "checkmark.circle.fill").accessibilityLabel("Selected") }
                            Button { editingWallet = wallet } label: { Image(systemName: "ellipsis") }
                                .buttonStyle(.borderless).accessibilityLabel("Manage \(wallet.name)").disabled(model.busy)
                        }
                        WalletAddressActions(address: wallet.address)
                    }.padding(.vertical, 6)
                }
            }
            Section {
                NavigationLink { AddressBookView(model: model) } label: { Label("Address book", systemImage: "person.crop.rectangle.stack") }
                Button("Add Wallet", systemImage: "plus") { addingWallet = true }
            }
        }
        .scrollContentBackground(.hidden)
        .background(NocksterPalette.background)
        .navigationTitle("Nockster")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                HStack(spacing: 8) {
                    Image("NocksterMark").renderingMode(.template).resizable().scaledToFit().frame(width: 26, height: 26)
                    Text("Nockster").font(.custom("Nokora-ExtraBold", size: 24, relativeTo: .title2)).tracking(-0.48)
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button("Lock Wallet", systemImage: "lock") { model.lock() }
            }
        }
        .refreshable { _ = await model.perform("refresh") }
        .task { _ = await model.perform("refresh") }
        .sheet(item: $editingWallet) { wallet in
            NavigationStack { ManageWalletView(model: model, wallet: wallet) }
        }
        .sheet(isPresented: $addingWallet) {
            NavigationStack {
                SetupView(model: model)
                    .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { addingWallet = false } } }
            }
        }
    }
}

struct SendView: View {
    @ObservedObject var model: WalletModel
    @State private var recipients = [PaymentRecipient(address: "", amount: "")]
    @State private var bridgeMode = false
    @State private var bridgeDestination = ""
    @State private var bridgeAmount = ""
    @State private var privateOutputs = false
    @State private var preview: PaymentPreview?
    @State private var submitted: String?
    @State private var inNicks = false
    @State private var contactRecipient: Int?
    @State private var choosingContact = false
    @State private var contactFilter = ""
    @State private var editingRecipient: Int?
    @State private var scanningRecipient: Int?
    private var filteredContacts: [AddressAlias] {
        let filter = contactFilter.trimmingCharacters(in: .whitespacesAndNewlines)
        return model.snapshot.contacts.filter { filter.isEmpty || $0.alias.localizedCaseInsensitiveContains(filter) || $0.address.localizedCaseInsensitiveContains(filter) }
    }

    private func changeUnit(_ nicks: Bool) {
        guard nicks != inNicks else { return }
        Task {
            let inputs = recipients.map { ["amount": $0.amount] }
            if let amounts = await model.perform("convertAmounts", ["amountUnit": inNicks ? "nicks" : "nock", "recipients": inputs])?.amounts {
                for index in recipients.indices { recipients[index].amount = amounts[index] }
                inNicks = nicks
                preview = nil
            }
        }
    }

    var body: some View {
        composeForm
        .navigationTitle("Send")
        .toolbar { ToolbarItem(placement: .topBarTrailing) {
            Button("Reset") { bridgeDestination = ""; bridgeAmount = ""; recipients = [PaymentRecipient(address: "", amount: "")]; privateOutputs = false; inNicks = false; preview = nil; editingRecipient = nil }.disabled(model.busy)
        } }
        .sheet(isPresented: Binding(get: { scanningRecipient != nil }, set: { if !$0 { scanningRecipient = nil } })) {
            NavigationStack {
                QRScannerView { value in
                    guard let index = scanningRecipient, recipients.indices.contains(index) else { return }
                    scanningRecipient = nil
                    Task {
                        if let address = await model.perform("scanAddress", ["key": value])?.address {
                            recipients[index].address = address
                            editingRecipient = nil
                        }
                    }
                } onError: { message in scanningRecipient = nil; model.error = message }
                .navigationTitle("Scan address")
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { scanningRecipient = nil } } }
            }
        }
        .onAppear { consumeHandoff() }
        .onChange(of: model.pendingRecipients) { _, _ in consumeHandoff() }
        .sheet(isPresented: $choosingContact) {
            NavigationStack {
                List {
                    if model.snapshot.contacts.isEmpty {
                        Text(model.busy ? "Loading addresses…" : "Your address book is empty. Add addresses from the Wallet page.")
                    }
                    if !model.snapshot.contacts.isEmpty && filteredContacts.isEmpty { Text("No matching addresses") }
                    ForEach(filteredContacts) { contact in
                        Button {
                            if let index = contactRecipient, recipients.indices.contains(index) { recipients[index].address = contact.address; editingRecipient = nil }
                            choosingContact = false
                        } label: {
                            VStack(alignment: .leading) {
                                Text(contact.alias).font(.headline)
                                Text(contact.address).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }.navigationTitle("Choose an address")
                    .searchable(text: $contactFilter, placement: .navigationBarDrawer(displayMode: .always), prompt: "Nickname or address")
                    .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { choosingContact = false } } }
                    .task { _ = await model.perform("contacts") }
            }
        }
        .sheet(item: $preview) { payment in reviewPayment(payment) }
        .alert("Transaction Submitted", isPresented: Binding(get: { submitted != nil }, set: { if !$0 { submitted = nil } })) {
            Button("Done") { submitted = nil }
        } message: { Text("Track confirmation in Activity.\n\(submitted ?? "")") }
    }
    private var composeForm: some View {
        Form {
            Section("From") {
                LabeledContent(model.snapshot.active?.name ?? "Wallet", value: "\(model.snapshot.active?.balance ?? "—") NOCK")
            }
            Picker("Send mode", selection: $bridgeMode) {
                Text("Send NOCK").tag(false)
                Text("Bridge to Base").tag(true)
            }.pickerStyle(.segmented).disabled(model.busy)
            if bridgeMode {
                Section {
                    TextField("Receiving Base address (0x…)", text: $bridgeDestination, axis: .vertical)
                        .font(.system(.body, design: .monospaced))
                        .textInputAutocapitalization(.never).autocorrectionDisabled().disabled(model.busy)
                    TextField("Amount in NOCK", text: $bridgeAmount).keyboardType(.decimalPad).disabled(model.busy)
                    UsdSubtitle(value: composeUsd(bridgeAmount, inNicks: false, price: model.snapshot.usdPerNock))
                } header: { Text("Bridge to Base") } footer: {
                    Text("Minimum 100,000 NOCK. Receive wrapped NOCK on Base after at least 400 Nockchain confirmations and bridge processing. The protocol deducts approximately 0.3% from the payout. Verify that you control the Base address; deposits cannot be reversed.")
                }
            } else {
                regularPaymentFields
            }
            Section {
                Button {
                    Task { await preparePayment() }
                } label: {
                    HStack { Text(bridgeMode ? "Review Bridge" : "Review Payment"); Spacer(); if model.busy { ProgressView() } else { Image(systemName: "arrow.right") } }
                }
                .disabled(model.busy || (bridgeMode ? bridgeDestination.isEmpty || bridgeAmount.isEmpty : recipients.contains { $0.address.isEmpty || $0.amount.isEmpty }))
            }
        }
    }

    private func preparePayment() async {
        var fields: [String: Any]
        if bridgeMode {
            inNicks = false
            fields = ["bridge": ["destination": bridgeDestination, "amount": bridgeAmount]]
        } else {
            let inputs = recipients.map { ["address": $0.address, "amount": $0.amount] }
            fields = ["recipients": inputs, "amountUnit": inNicks ? "nicks" : "nock", "privateOutputs": privateOutputs]
        }
        if let reply = await model.perform("prepare", fields) { preview = reply.preview }
    }

    @ViewBuilder private var regularPaymentFields: some View {
            Section("Amount unit") {
                Picker("Unit", selection: Binding(get: { inNicks }, set: { changeUnit($0) })) {
                    Text("NOCK").tag(false)
                    Text("NICKS").tag(true)
                }.pickerStyle(.segmented).disabled(model.busy)
            }
            ForEach(recipients.indices, id: \.self) { index in
                Section("Recipient \(index + 1)") {
                    if let contact = model.snapshot.contacts.first(where: { $0.address == recipients[index].address.trimmingCharacters(in: .whitespacesAndNewlines) }) {
                        Label(contact.alias, systemImage: "person.crop.circle").font(.headline)
                    }
                    if editingRecipient != index && model.snapshot.contacts.contains(where: { $0.address == recipients[index].address.trimmingCharacters(in: .whitespacesAndNewlines) }) {
                        Button { editingRecipient = index } label: {
                            HStack {
                                Text("\(recipients[index].address.prefix(8))…\(recipients[index].address.suffix(8))").font(.system(.body, design: .monospaced))
                                Spacer()
                                Image(systemName: "pencil").accessibilityLabel("Edit address")
                            }
                        }.disabled(model.busy)
                    } else {
                        TextField("Nockchain address", text: $recipients[index].address, axis: .vertical)
                            .font(.system(.body, design: .monospaced))
                            .textInputAutocapitalization(.never).autocorrectionDisabled().disabled(model.busy)
                    }
                    HStack {
                        PasteButton(payloadType: String.self) { values in
                            if let value = values.first { recipients[index].address = value.trimmingCharacters(in: .whitespacesAndNewlines) }
                        }
                        Button("Address book", systemImage: "person.crop.rectangle.stack") {
                            contactFilter = ""
                            contactRecipient = index
                            choosingContact = true
                        }
                        Button { UIPasteboard.general.string = recipients[index].address.trimmingCharacters(in: .whitespacesAndNewlines) } label: { Image(systemName: "doc.on.doc").accessibilityLabel("Copy address") }
                            .disabled(recipients[index].address.isEmpty)
                        Button { Task {
                            model.cameraPermissionPrompt = true
                            let granted = await AVCaptureDevice.requestAccess(for: .video)
                            model.cameraPermissionPrompt = false
                            if granted && model.snapshot.unlocked { scanningRecipient = index }
                            else if !granted { model.error = "Allow camera access in Settings to scan a QR code" }
                        } } label: { Image(systemName: "qrcode.viewfinder").accessibilityLabel("Scan QR code") }
                    }.disabled(model.busy)
                    if index == recipients.count - 1 {
                        Button("Add Recipient", systemImage: "plus") { recipients.append(PaymentRecipient(address: "", amount: "")) }.disabled(recipients.count >= 16 || model.busy)
                    }
                    TextField(inNicks ? "Amount in nicks" : "Amount in NOCK", text: $recipients[index].amount)
                        .keyboardType(inNicks ? .numberPad : .decimalPad).disabled(model.busy)
                    UsdSubtitle(value: composeUsd(recipients[index].amount, inNicks: inNicks, price: model.snapshot.usdPerNock))
                    if recipients.count > 1 { Button("Remove Recipient", role: .destructive) { recipients.remove(at: index); editingRecipient = nil }.disabled(model.busy) }
                }
            }
            Section {
                Toggle("Private outputs", isOn: $privateOutputs)
            } footer: { Text("Review the exact network fee before submitting. Private outputs omit output lock disclosures.") }
    }

    private func reviewPayment(_ payment: PaymentPreview) -> some View {
            NavigationStack {
                Form {
                    if let bridge = payment.bridge {
                        Section("Receive on Base") {
                            Text(bridge.destination).font(.system(.footnote, design: .monospaced)).textSelection(.enabled)
                            LabeledContent("Protocol fee (≈0.3%)", value: "\(bridge.protocolFee) NOCK")
                            LabeledContent("Expected on Base", value: "\(bridge.expectedReceived) NOCK")
                            Text("The protocol fee is deducted from the deposit. Wait at least 400 Nockchain confirmations plus bridge processing. Check the complete receiving address before confirming.").font(.footnote)
                        }
                    } else {
                    Section("To") {
                        ForEach(Array(payment.recipients.enumerated()), id: \.offset) { _, recipient in
                            VStack(alignment: .leading, spacing: 8) {
                                if let contact = model.snapshot.contacts.first(where: { $0.address == recipient.address }) {
                                    Label(contact.alias, systemImage: "person.crop.circle").font(.headline)
                                }
                                Text(recipient.address).font(.system(.footnote, design: .monospaced)).textSelection(.enabled)
                                Text("\(inNicks ? (recipient.amountNicks ?? "—") : recipient.amount) \(inNicks ? "NICKS" : "NOCK")").font(.headline)
                                UsdSubtitle(value: recipient.amountUsd)
                            }.padding(.vertical, 8)
                        }
                    }
                    }
                    Section {
                        VStack(alignment: .leading, spacing: 4) {
                            LabeledContent("Net sent", value: "\(inNicks ? payment.netSentNicks : payment.netSent) \(inNicks ? "NICKS" : "NOCK")")
                            UsdSubtitle(value: payment.netSentUsd)
                        }
                        VStack(alignment: .leading, spacing: 4) {
                            LabeledContent("Network fee", value: "\(inNicks ? payment.feeNicks : payment.fee) \(inNicks ? "NICKS" : "NOCK")")
                            UsdSubtitle(value: payment.feeUsd)
                        }
                        VStack(alignment: .leading, spacing: 4) {
                            LabeledContent("Total", value: "\(inNicks ? payment.totalNicks : payment.total) \(inNicks ? "NICKS" : "NOCK")")
                            UsdSubtitle(value: payment.totalUsd)
                        }
                        LabeledContent("Private outputs", value: payment.privateOutputs ? "On" : "Off")
                    }
                    Section {
                        PaymentAuthenticationView(model: model) { password in
                            let reply = await model.confirmSubmission("send", fields: ["previewId": payment.id], password: password)
                            if let txId = reply?.txId { preview = nil; submitted = txId; bridgeAmount = ""; recipients = [PaymentRecipient(address: "", amount: "")] }
                        }
                    } footer: { Text("This sends funds on Nockchain mainnet. Check every address and amount.") }
                }
                .navigationTitle(payment.bridge == nil ? "Review Payment" : "Review Bridge")
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { preview = nil }.disabled(model.busy) } }
                .interactiveDismissDisabled(model.busy)
            }
    }

    private func consumeHandoff() {
        if !model.pendingRecipients.isEmpty {
            inNicks = false
            bridgeMode = false
            recipients = model.pendingRecipients
            model.pendingRecipients = []
            preview = nil
        }
    }
}

struct ReceiveView: View {
    @ObservedObject var model: WalletModel
    @Environment(\.colorScheme) private var colorScheme
    var address: String { model.snapshot.active?.address ?? "" }
    var body: some View {
        ScrollView {
            VStack(spacing: 28) {
                Text("Receive NOCK").font(.title.bold())
                if let image = qrImage(address) {
                    Image(uiImage: image).interpolation(.none).resizable().scaledToFit()
                        .frame(maxWidth: 260).padding(24).background(.white, in: RoundedRectangle(cornerRadius: 28))
                        .accessibilityLabel("QR code for your Nockchain address")
                }
                Text(model.snapshot.active?.name ?? "Wallet").font(.headline)
                Text(address).font(.system(.callout, design: .monospaced))
                    .multilineTextAlignment(.center).textSelection(.enabled)
                HStack(spacing: 16) {
                    Button("Copy", systemImage: "doc.on.doc") { UIPasteboard.general.string = address }
                        .buttonStyle(.glass)
                    ShareLink(item: address) {
                        Label("Share", systemImage: "square.and.arrow.up")
                            .foregroundStyle(colorScheme == .dark ? Color.black : Color.white)
                    }
                        .buttonStyle(.glassProminent)
                        .tint(colorScheme == .dark ? .white : .black)
                }.controlSize(.large)
                Text("This is a layer 1 Nockchain address").font(.footnote).foregroundStyle(.secondary)
                if let url = walletExplorerURL(address) {
                    Link(destination: url) { Label("View on Nockblocks", systemImage: "arrow.up.right.square") }.font(.footnote).tint(.secondary)
                }
            }.padding(28).frame(maxWidth: 560).frame(maxWidth: .infinity)
        }.navigationTitle("Receive").navigationBarTitleDisplayMode(.inline)
    }
    private func qrImage(_ value: String) -> UIImage? {
        guard !value.isEmpty else { return nil }
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(value.utf8)
        guard let output = filter.outputImage?.transformed(by: CGAffineTransform(scaleX: 10, y: 10)),
              let image = CIContext().createCGImage(output, from: output.extent) else { return nil }
        return UIImage(cgImage: image)
    }
}

struct TransactionDetailView: View {
    @ObservedObject var model: WalletModel
    let txId: String
    @Environment(\.dismiss) private var dismiss
    @State private var clearing = false
    var body: some View {
        Group {
            if let transaction = model.snapshot.history.first(where: { $0.id == txId }) {
                Form {
                    Section {
                        VStack(alignment: .leading, spacing: 4) { LabeledContent("Amount", value: "\(groupedAmount(transaction.amount)) NOCK"); UsdSubtitle(value: transaction.amountUsd) }
                        VStack(alignment: .leading, spacing: 4) { LabeledContent("Total fees", value: "\(groupedAmount(transaction.fee)) NOCK"); UsdSubtitle(value: transaction.feeUsd) }
                        LabeledContent("Status", value: transaction.statusLabel)
                        Text(transaction.date, style: .date)
                    }
                    if let bridge = transaction.bridge {
                        Section("Bridge to Base") {
                            CopyAddressView(label: "Base destination", address: bridge.destination)
                            LabeledContent("Bridge deposit", value: "\(groupedAmount(bridge.amount)) NOCK")
                            LabeledContent("Bridge protocol fee (≈0.3%)", value: "\(groupedAmount(bridge.protocolFee)) NOCK")
                            LabeledContent("Expected on Base", value: "\(groupedAmount(bridge.expectedReceived)) NOCK")
                            Text("L1 confirmation does not mean delivery on Base. The bridge waits 400 blocks before processing.").font(.footnote)
                            if let url = URL(string: "https://basescan.org/address/\(bridge.destination)") { Link("View destination on Base", destination: url) }
                        }
                    }
                    Section(transaction.bridge != nil ? "To Base" : transaction.direction == "self" ? "Own address" : transaction.direction == "sent" ? "To" : "From") {
                        ForEach(transaction.parties) { party in CopyAddressView(label: party.label, address: party.address) }
                    }
                    if let height = transaction.blockHeight { Section { LabeledContent("Block", value: height.formatted(.number.locale(Locale(identifier: "en_US")))) } }
                    Section("Transaction ID") {
                        CopyAddressView(label: nil, address: transaction.id)
                        if let url = URL(string: "https://nockblocks.com/tx/\(transaction.id)") {
                            ShareLink(item: url)
                            Link("View on Nockblocks", destination: url)
                        }
                    }
                    if transaction.canClear {
                        Section {
                            if let detail = transaction.pendingDetail { Text(detail).foregroundStyle(.secondary) }
                            Button("Clear unconfirmed transaction", role: .destructive) { clearing = true }.disabled(model.busy)
                            if let failure = transaction.error { Text(failure).foregroundStyle(.secondary) }
                            DisclosureGroup("Retry Submission") {
                                PaymentAuthenticationView(model: model) { password in
                                    _ = await model.confirmSubmission("retry", fields: ["txId": transaction.id], password: password)
                                }
                            }.disabled(model.busy)
                        } footer: { Text("Retries submit the same signed transaction. Its inputs remain reserved until confirmation.") }
                    }
                }.navigationTitle("Transaction").navigationBarTitleDisplayMode(.inline)
            }
        }
        .onChange(of: model.snapshot.history.map(\.id)) { _, ids in
            if !ids.contains(txId) { dismiss() }
        }
        .alert("Clear unconfirmed transaction?", isPresented: $clearing) {
            Button("Cancel", role: .cancel) { }
            Button("Clear", role: .destructive) {
                Task {
                    if await model.perform("clearPending", ["walletId": model.snapshot.activeId ?? "", "txId": txId, "confirmation": txId]) != nil { dismiss() }
                }
            }
        } message: {
            Text("This releases its reserved funds locally. It does not cancel the transaction on the network; it could still confirm. Check the explorer before sending again.")
        }
    }
}

struct ActivityView: View {
    @ObservedObject var model: WalletModel
    var body: some View {
        List {
            if let wallet = model.snapshot.active {
                Section {
                    Text(wallet.name).font(.headline)
                    Text(shortWalletAddress(wallet.address)).font(.caption.monospaced()).foregroundStyle(.secondary)
                }
            }
            if model.snapshot.history.isEmpty {
                ContentUnavailableView("No Transactions", systemImage: "clock", description: Text("Pull to refresh your wallet activity."))
            }
            ForEach(model.snapshot.history) { transaction in
                NavigationLink {
                    TransactionDetailView(model: model, txId: transaction.id)
                } label: {
                    HStack(spacing: 14) {
                        Image(systemName: transaction.direction == "sent" ? "arrow.up.right" : "arrow.down.left")
                            .font(.title2).frame(width: 28).foregroundStyle(transaction.direction == "sent" ? Color.primary : Color.teal)
                        VStack(alignment: .leading) {
                            Text(transaction.bridge != nil ? "Bridge to Base" : transaction.direction == "self" ? "Self transfer" : transaction.direction.capitalized).font(.caption).foregroundStyle(.secondary)
                            if let party = transaction.parties.first {
                                Text((transaction.bridge != nil ? "To Base: " : transaction.direction == "self" ? "Own: " : transaction.direction == "sent" ? "To: " : "From: ") + (party.label ?? "\(party.address.prefix(8))…\(party.address.suffix(8))") + (transaction.parties.count > 1 ? " +\(transaction.parties.count - 1)" : "")).font(.headline).lineLimit(1).truncationMode(.tail)
                            }
                            Text(transaction.date, style: .date).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        VStack(alignment: .trailing) {
                            Text("\(transaction.direction == "sent" ? "−" : transaction.direction == "received" ? "+" : "")\(groupedAmount(transaction.amount)) NOCK").foregroundStyle(transaction.direction == "sent" ? Color.primary : Color.teal).lineLimit(1).minimumScaleFactor(0.6)
                            UsdSubtitle(value: transaction.amountUsd)
                            Text(transaction.statusLabel).font(.caption).foregroundStyle(.secondary)
                        }
                        if let party = transaction.parties.first {
                            Button { UIPasteboard.general.string = party.address } label: { Image(systemName: "doc.on.doc").accessibilityLabel("Copy address") }.buttonStyle(.borderless)
                        }
                    }.padding(.vertical, 5)
                }
            }
        }
        .navigationTitle("Activity")
        .refreshable { _ = await model.perform("refresh") }
        .task(id: model.snapshot.activeId) {
            while !Task.isCancelled {
                if !model.busy { _ = await model.perform("refresh") }
                do { try await Task.sleep(for: .seconds(30)) } catch { break }
            }
        }
    }
}

struct WalletSettingsView: View {
    @ObservedObject var model: WalletModel
    @State private var name = ""
    @State private var deviceUnlockPassword = ""
    @FocusState private var deviceUnlockPasswordFocused: Bool

    private var nameExists: Bool {
        model.snapshot.wallets.contains { $0.id != model.snapshot.activeId && $0.name == name.trimmingCharacters(in: .whitespacesAndNewlines) }
    }

    private func enableDeviceUnlock() {
        guard !deviceUnlockPassword.isEmpty, !model.busy else { return }
        deviceUnlockPasswordFocused = false
        let password = deviceUnlockPassword
        deviceUnlockPassword = ""
        Task { await model.enableDeviceUnlock(password: password) }
    }

    var body: some View {
        Form {
            Section("Current wallet") {
                TextField("Wallet name", text: $name).foregroundStyle(nameExists ? Color.red : Color.primary)
                if nameExists { Text("A wallet with this name already exists").font(.caption).foregroundStyle(.red) }
                Button("Rename Wallet") { Task { _ = await model.perform("rename", ["name": name]) } }
                    .disabled(nameExists || name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.busy)
            }
            Section {
                if model.deviceUnlockEnabled {
                    Button("Disable device unlock", role: .destructive) {
                        do { try DeviceUnlock.disable(); model.deviceUnlockEnabled = false }
                        catch { model.error = error.localizedDescription }
                    }.disabled(model.busy)
                } else {
                    SecureField("Confirm wallet password", text: $deviceUnlockPassword).textContentType(.password)
                        .focused($deviceUnlockPasswordFocused).submitLabel(.done).onSubmit { enableDeviceUnlock() }
                        .disabled(model.busy)
                    Button("Continue to device verification", systemImage: "faceid") { enableDeviceUnlock() }
                        .disabled(deviceUnlockPassword.isEmpty || model.busy)
                }
            } header: { Text("Device passcode or biometrics") }
              footer: {
                  Text("Use your device passcode from the system prompt if you prefer it to biometrics. Your wallet password still works.")
                  if !model.deviceUnlockEnabled {
                      Text("Enter your wallet password to enable unlock with this device’s passcode, Face ID, or Touch ID.")
                  }
            }
            Section {
                Button { model.lock() } label: {
                    Label("Lock wallet", systemImage: "lock").frame(maxWidth: .infinity, minHeight: 36)
                }
                .buttonStyle(.borderedProminent).tint(.black).foregroundStyle(.white)
                .listRowInsets(EdgeInsets()).listRowBackground(Color.clear)
            }
            Section("Network") {
                LabeledContent("Network", value: "Nockchain mainnet")
                Link("Nockblocks Explorer", destination: URL(string: "https://nockblocks.com")!)
            }
            Section("Support") {
                NavigationLink("Send feedback") { FeedbackView(model: model) }
                Link("Privacy Policy", destination: URL(string: "https://nockster.com/wallet/privacy")!)
            }
            Section {
                LabeledContent("Nockster", value: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "")
            }
            Section {
                Link(destination: URL(string: "https://swps.io")!) {
                    VStack(spacing: 12) {
                        Text("Developed by").font(.caption).foregroundStyle(.secondary)
                        Image("SWPSLogo").renderingMode(.template).resizable().scaledToFit().frame(maxWidth: 240)
                            .foregroundStyle(.primary).accessibilityLabel("South Western Pool Supply")
                    }.padding(.vertical, 24).frame(maxWidth: .infinity)
                }.listRowBackground(Color.clear)
            }
        }
        .scrollContentBackground(.hidden)
        .background(NocksterPalette.background)
        .navigationTitle("Settings")
        .onAppear { name = model.snapshot.active?.name ?? "" }
    }
}

struct FeedbackView: View {
    @ObservedObject var model: WalletModel
    @Environment(\.openURL) private var openURL
    @State private var emailUnavailable = false

    private func composeEmail() {
        var url = URLComponents()
        url.scheme = "mailto"
        url.path = "howdy@swps.io"
        url.queryItems = [
            URLQueryItem(name: "subject", value: model.feedbackSubject.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "Nockster feedback" : model.feedbackSubject),
            URLQueryItem(name: "body", value: model.feedbackMessage)
        ]
        guard let destination = url.url else { emailUnavailable = true; return }
        openURL(destination) { accepted in emailUnavailable = !accepted }
    }

    var body: some View {
        Form {
            Section {
                TextField("Subject (optional)", text: $model.feedbackSubject)
                TextField("What would you like us to know?", text: $model.feedbackMessage, axis: .vertical)
                    .lineLimit(6...14)
                    .accessibilityLabel("Feedback message")
            } header: { Text("Your feedback") }
              footer: { Text("Please leave out recovery phrases, private keys, and passwords.") }
            Section {
                Button("Open email", systemImage: "envelope") { composeEmail() }
                    .disabled(model.feedbackMessage.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            } footer: { Text("Opens a draft to howdy@swps.io in your email app. You review and send it there.") }
        }
        .navigationTitle("Send feedback")
        .alert("No email app available", isPresented: $emailUnavailable) {
            Button("OK", role: .cancel) {}
        } message: { Text("Set up an email app, or copy your message and email howdy@swps.io. Your draft is still here.") }
    }
}


struct AddressBookView: View {
    @ObservedObject var model: WalletModel
    @Environment(\.dismiss) private var dismiss
    @State private var alias = ""
    @State private var address = ""
    var body: some View {
        Form {
            Section("Add or update an address") {
                TextField("Nickname", text: $alias)
                TextField("Nockchain address", text: $address).textInputAutocapitalization(.never).autocorrectionDisabled()
                Button("Save Address") { Task {
                    if await model.perform("saveContact", ["name": alias, "address": address]) != nil { alias = ""; address = "" }
                } }.disabled(alias.isEmpty || address.isEmpty || model.busy)
            }
            Section("Synced with Nockblocks") {
                ForEach(model.snapshot.contacts) { contact in
                    VStack(alignment: .leading) {
                        Text(contact.alias).font(.headline)
                        Text(contact.address).font(.caption).textSelection(.enabled)
                        Button("Use Address") { model.pendingRecipients = [PaymentRecipient(address: contact.address, amount: "")]; dismiss() }
                        Button("Edit Nickname") { alias = contact.alias; address = contact.address }
                        Button("Delete", role: .destructive) { Task { _ = await model.perform("deleteContact", ["contactId": contact.id]) } }
                    }
                }
                Button("Refresh") { Task { _ = await model.perform("contacts") } }.disabled(model.busy)
            }
        }.navigationTitle("Address Book").task { _ = await model.perform("contacts") }
    }
}

private enum NocksterPalette {
    static let background = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 20 / 255, green: 20 / 255, blue: 20 / 255, alpha: 1)
            : .white
    })
}

private struct PaymentAuthenticationView: View {
    @ObservedObject var model: WalletModel
    let submit: (String?) async -> Void
    @State private var password = ""
    @FocusState private var focused: Bool
    private func confirm() {
        guard !password.isEmpty, !model.busy else { return }
        let credential = password
        password = ""
        focused = false
        Task { await submit(credential) }
    }
    var body: some View {
        SecureField("Wallet password", text: $password).textContentType(.password)
            .focused($focused).submitLabel(.send).onSubmit { confirm() }.disabled(model.busy)
        Button("Confirm and Send") { confirm() }.disabled(password.isEmpty || model.busy)
        if model.deviceUnlockEnabled {
            Button("Confirm with device unlock", systemImage: "faceid") {
                password = ""; focused = false
                Task { await submit(nil) }
            }.disabled(model.busy)
        }
        if let error = model.error { Text(error).foregroundStyle(.red) }
    }
}

private struct QRScannerView: UIViewControllerRepresentable {
    let onScan: (String) -> Void
    let onError: (String) -> Void
    func makeUIViewController(context: Context) -> QRScannerController {
        let controller = QRScannerController()
        controller.onScan = onScan
        controller.onError = onError
        return controller
    }
    func updateUIViewController(_ controller: QRScannerController, context: Context) {}
}

private final class QRScannerController: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
    var onScan: (String) -> Void = { _ in }
    var onError: (String) -> Void = { _ in }
    private let session = AVCaptureSession()
    private let cameraQueue = DispatchQueue(label: "io.swps.nockster.qr-camera")
    private var previewLayer: AVCaptureVideoPreviewLayer?
    private var delivered = false
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        guard let camera = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: camera), session.canAddInput(input) else {
            DispatchQueue.main.async { self.onError("Camera is unavailable on this device") }; return
        }
        session.addInput(input)
        let output = AVCaptureMetadataOutput()
        guard session.canAddOutput(output) else {
            DispatchQueue.main.async { self.onError("QR scanning is unavailable") }; return
        }
        session.addOutput(output)
        output.setMetadataObjectsDelegate(self, queue: .main)
        output.metadataObjectTypes = [.qr]
        let layer = AVCaptureVideoPreviewLayer(session: session)
        layer.videoGravity = .resizeAspectFill
        view.layer.addSublayer(layer)
        previewLayer = layer
        cameraQueue.async { self.session.startRunning() }
    }
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        previewLayer?.frame = view.bounds
    }
    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        cameraQueue.async { self.session.stopRunning() }
    }
    func metadataOutput(_ output: AVCaptureMetadataOutput, didOutput objects: [AVMetadataObject], from connection: AVCaptureConnection) {
        guard !delivered, let value = (objects.first as? AVMetadataMachineReadableCodeObject)?.stringValue else { return }
        delivered = true
        cameraQueue.async { self.session.stopRunning() }
        onScan(value)
    }
}


private struct CopyAddressView: View {
    let label: String?
    let address: String
    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 6) {
                if let label { Text(label).font(.headline).lineLimit(1).truncationMode(.tail) }
                Text(address.count > 20 ? "\(address.prefix(8))…\(address.suffix(8))" : address).font(.system(.footnote, design: .monospaced))
            }
            Spacer()
            Button { UIPasteboard.general.string = address } label: { Image(systemName: "doc.on.doc").accessibilityLabel("Copy \(address)") }.buttonStyle(.borderless)
        }
    }
}


private func composeUsd(_ input: String, inNicks: Bool, price: Double?) -> String? {
    guard let amount = Double(input.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: ",", with: "")),
          amount.isFinite, amount >= 0, !inNicks || amount.rounded() == amount,
          let price, price.isFinite, price > 0 else { return nil }
    let value = amount / (inNicks ? 65536 : 1) * price
    guard value.isFinite else { return nil }
    if value > 0 && value < 0.001 { return "<0.001 USD" }
    let formatter = NumberFormatter()
    formatter.locale = Locale(identifier: "en_US")
    formatter.numberStyle = .decimal
    formatter.minimumFractionDigits = value >= 1 ? 2 : 3
    formatter.maximumFractionDigits = formatter.minimumFractionDigits
    guard let text = formatter.string(from: NSNumber(value: value)) else { return nil }
    return "~\(text) USD"
}

private struct UsdSubtitle: View {
    let value: String?
    var body: some View {
        if let value { Text(value).font(.caption).foregroundStyle(.secondary) }
    }
}
