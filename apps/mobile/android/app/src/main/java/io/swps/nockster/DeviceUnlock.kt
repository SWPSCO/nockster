package io.swps.nockster

import android.content.Context
import android.content.pm.PackageManager
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyInfo
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.suspendCancellableCoroutine
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

class HardwareUnlockUnavailable : IllegalStateException(
    "Hardware-protected device unlock is unavailable on this device. PIN and biometric unlock cannot be enabled. " +
        "Your wallet remains encrypted with your wallet password. Use a strong, unique wallet password."
)

class DeviceUnlock(private val activity: FragmentActivity) {
    private val alias = "com.fletch.wallet.unlock"
    private val preferences = activity.getSharedPreferences("device-unlock", Context.MODE_PRIVATE)
    private fun keyStore() = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    val enabled: Boolean get() = preferences.contains("ciphertext") && keyStore().containsAlias(alias)

    fun disable() {
        check(preferences.edit().clear().commit()) { "Unable to remove device unlock" }
        keyStore().deleteEntry(alias)
    }

    private suspend fun authenticate(cipher: Cipher, title: String = "Unlock Nockster", subtitle: String = "Authenticate to use your device-protected wallet credential"): Cipher = suspendCancellableCoroutine { continuation ->
        val prompt = BiometricPrompt(activity, ContextCompat.getMainExecutor(activity),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    val authenticated = result.cryptoObject?.cipher
                    if (continuation.isActive) {
                        if (authenticated == null) continuation.resumeWithException(IllegalStateException("Missing authenticated cipher"))
                        else continuation.resume(authenticated)
                    }
                }
                override fun onAuthenticationError(code: Int, message: CharSequence) {
                    if (continuation.isActive) continuation.resumeWithException(IllegalStateException(message.toString()))
                }
            })
        continuation.invokeOnCancellation { prompt.cancelAuthentication() }
        prompt.authenticate(BiometricPrompt.PromptInfo.Builder()
            .setTitle(title)
            .setSubtitle(subtitle)
            .setAllowedAuthenticators((BiometricManager.Authenticators.BIOMETRIC_STRONG or BiometricManager.Authenticators.DEVICE_CREDENTIAL))
            .build(), BiometricPrompt.CryptoObject(cipher))
    }

    suspend fun enable(secret: String) {
        check(BiometricManager.from(activity).canAuthenticate((BiometricManager.Authenticators.BIOMETRIC_STRONG or BiometricManager.Authenticators.DEVICE_CREDENTIAL)) == BiometricManager.BIOMETRIC_SUCCESS) {
            "Set up a device screen lock in Android Settings"
        }
        val cipher = withContext(Dispatchers.IO) {
            disable()
            val specification = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setUserAuthenticationRequired(true)
                .setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL)
                .setIsStrongBoxBacked(activity.packageManager.hasSystemFeature(PackageManager.FEATURE_STRONGBOX_KEYSTORE))
                .build()
            val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
            generator.init(specification)
            val key = generator.generateKey()
            val info = SecretKeyFactory.getInstance(key.algorithm, "AndroidKeyStore").getKeySpec(key, KeyInfo::class.java) as KeyInfo
            if ((info.securityLevel != KeyProperties.SECURITY_LEVEL_STRONGBOX &&
                    info.securityLevel != KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT) ||
                !info.isUserAuthenticationRequirementEnforcedBySecureHardware) {
                keyStore().deleteEntry(alias)
                throw HardwareUnlockUnavailable()
            }
            Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key) }
        }
        try {
            val authenticated = authenticate(cipher, "Enable device unlocking", "Confirm with biometrics or your device screen lock")
            withContext(Dispatchers.IO) {
                val bytes = secret.toByteArray(Charsets.UTF_8)
                val ciphertext = try { authenticated.doFinal(bytes) } finally { bytes.fill(0) }
                check(preferences.edit()
                    .putString("ciphertext", Base64.encodeToString(ciphertext, Base64.NO_WRAP))
                    .putString("iv", Base64.encodeToString(authenticated.iv, Base64.NO_WRAP)).commit()) { "Unable to save device unlock" }
            }
        } catch (error: Exception) {
            withContext(Dispatchers.IO) { disable() }
            throw error
        }
    }

    suspend fun secret(title: String = "Unlock Nockster"): String {
        val cipher = withContext(Dispatchers.IO) {
            val key = keyStore().getKey(alias, null) as? SecretKey ?: error("Use your wallet password and enable device unlock again")
            val iv = Base64.decode(preferences.getString("iv", null) ?: error("Missing unlock credential"), Base64.NO_WRAP)
            Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, iv)) }
        }
        val authenticated = authenticate(cipher, title)
        return withContext(Dispatchers.IO) {
            val ciphertext = Base64.decode(preferences.getString("ciphertext", null) ?: error("Missing unlock credential"), Base64.NO_WRAP)
            val plaintext = authenticated.doFinal(ciphertext)
            try { plaintext.toString(Charsets.UTF_8) } finally { plaintext.fill(0) }
        }
    }
}
