import Foundation
import Security
import LocalAuthentication

// Secure Enclave wraps the vault master key; Nockchain signing runs in the wallet engine.
enum DeviceUnlock {
    private static let tag = Data("com.fletch.wallet.unlock".utf8)
    private static let service = "com.fletch.wallet.biometric-unlock"
    private static let algorithm = SecKeyAlgorithm.eciesEncryptionCofactorX963SHA256AESGCM

    static var enabled: Bool {
        var query = envelopeQuery
        query[kSecReturnData as String] = false
        return SecItemCopyMatching(query as CFDictionary, nil) == errSecSuccess
    }

    private static var envelopeQuery: [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: "wallet"]
    }

    private static var keyQuery: [String: Any] {
        [kSecClass as String: kSecClassKey,
         kSecAttrApplicationTag as String: tag,
         kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom]
    }

    static func disable() throws {
        for query in [envelopeQuery, keyQuery] {
            let status = SecItemDelete(query as CFDictionary)
            guard status == errSecSuccess || status == errSecItemNotFound else {
                throw WalletFailure(message: "Unable to remove device unlock (\(status))")
            }
        }
    }

    static func enable(secret: String) async throws {
        try await Task.detached {
            let context = LAContext()
            defer { context.invalidate() }
            var authError: NSError?
            guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &authError) else {
                throw WalletFailure(message: authError?.localizedDescription ?? "Set up a device passcode in Settings")
            }
            // Authentication is also required during enrollment.
            try await context.evaluatePolicy(.deviceOwnerAuthentication,
                                             localizedReason: "Enable device unlocking with your device’s biometrics or screen-lock passcode or password")
            try disable()
            var error: Unmanaged<CFError>?
            guard let access = SecAccessControlCreateWithFlags(nil,
                kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly, [.privateKeyUsage, .userPresence], &error) else {
                throw error!.takeRetainedValue() as Error
            }
            let attributes: [String: Any] = [
                kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
                kSecAttrKeySizeInBits as String: 256,
                kSecAttrTokenID as String: kSecAttrTokenIDSecureEnclave,
                kSecPrivateKeyAttrs as String: [
                    kSecAttrIsPermanent as String: true,
                    kSecAttrApplicationTag as String: tag,
                    kSecAttrAccessControl as String: access
                ]
            ]
            guard let key = SecKeyCreateRandomKey(attributes as CFDictionary, &error),
                  let publicKey = SecKeyCopyPublicKey(key) else {
                throw WalletFailure(message: "Secure Enclave is unavailable. Use your wallet password on this device.")
            }
            guard let ciphertext = SecKeyCreateEncryptedData(publicKey, algorithm, Data(secret.utf8) as CFData, &error) else {
                try? disable()
                throw error!.takeRetainedValue() as Error
            }
            var query = envelopeQuery
            query[kSecValueData as String] = ciphertext
            query[kSecAttrAccessible as String] = kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly
            let status = SecItemAdd(query as CFDictionary, nil)
            guard status == errSecSuccess else {
                try? disable()
                throw WalletFailure(message: "Unable to save device unlock (\(status))")
            }
        }.value
    }

    static func secret(reason: String = "Unlock your Nockster wallet") async throws -> String {
        try await Task.detached {
            var query = envelopeQuery
            query[kSecReturnData as String] = true
            var item: CFTypeRef?
            guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
                  let ciphertext = item as? Data else {
                throw WalletFailure(message: "Device unlock is unavailable. Enter your wallet password.")
            }
            let context = LAContext()
            context.localizedReason = reason
            defer { context.invalidate() }
            var keyRequest = keyQuery
            keyRequest[kSecReturnRef as String] = true
            keyRequest[kSecUseAuthenticationContext as String] = context
            var keyItem: CFTypeRef?
            guard SecItemCopyMatching(keyRequest as CFDictionary, &keyItem) == errSecSuccess, let keyItem else {
                throw WalletFailure(message: "Use your wallet password, then enable device unlock again.")
            }
            let key = keyItem as! SecKey
            var error: Unmanaged<CFError>?
            guard let plaintext = SecKeyCreateDecryptedData(key, algorithm, ciphertext as CFData, &error),
                  let secret = String(data: plaintext as Data, encoding: .utf8) else {
                throw error?.takeRetainedValue() as Error? ?? WalletFailure(message: "Device authentication failed")
            }
            return secret
        }.value
    }
}
