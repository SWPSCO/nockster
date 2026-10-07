package io.swps.nockster

import android.net.Uri
import android.webkit.WebView
import androidx.compose.runtime.*
import kotlinx.coroutines.*
import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener
import java.util.UUID
import kotlin.coroutines.resume

fun JSONArray.objects(): List<JSONObject> = (0 until length()).map { getJSONObject(it) }
fun JSONObject.text(name: String, default: String = ""): String = if (isNull(name)) default else optString(name, default)

class WalletModel(private val webView: WebView, val deviceUnlock: DeviceUnlock, private val scope: CoroutineScope,
                  val screenCapture: ScreenCaptureSettings) {
    var snapshot by mutableStateOf(JSONObject())
    var ready by mutableStateOf(false)
    var busy by mutableStateOf(false)
    var error by mutableStateOf<String?>(null)
    var notice by mutableStateOf<String?>(null)
    var feedbackSubject by mutableStateOf("")
    var feedbackMessage by mutableStateOf("")
    var privateScreen by mutableStateOf(false)
    var session by mutableIntStateOf(0)
    var deviceUnlockEnabled by mutableStateOf(deviceUnlock.enabled)
    var deviceUnlockWarning by mutableStateOf<String?>(null)
    var pendingRecipients by mutableStateOf<List<Pair<String, String>>>(emptyList())
    private var authenticating = false
    var cameraPermissionPrompt = false
    private var foreground = false
    private var automaticUnlockPending = true
    private var lockJob: Job? = null
    val unlocked get() = snapshot.optBoolean("unlocked")
    val wallets get() = snapshot.optJSONArray("wallets")?.objects().orEmpty()
    val active get() = wallets.find { it.text("id") == snapshot.text("activeId") }
    val history get() = snapshot.optJSONArray("history")?.objects().orEmpty()

    private suspend fun javascript(script: String): Any? = suspendCancellableCoroutine { continuation ->
        webView.evaluateJavascript(script) { json ->
            if (continuation.isActive) continuation.resume(if (json == null || json == "null") null else JSONTokener(json).nextValue())
        }
    }

    suspend fun start() {
        repeat(100) {
            if (javascript("Boolean(window.nocksterNative)") == true) {
                try {
                    val reply = call("status")
                    snapshot = reply.getJSONObject("state")
                    error = reply.text("error").ifEmpty { null }
                    ready = true
                    automaticallyUnlock()
                }
                catch (failure: Exception) { error = failure.message }
                return
            }
            delay(200)
        }
        error = "The wallet engine could not start. Close and reopen Nockster."
    }

    private suspend fun call(action: String, fields: JSONObject = JSONObject()): JSONObject {
        val request = JSONObject(fields.toString()).put("action", action)
        val id = JSONObject.quote(UUID.randomUUID().toString())
        javascript("""
            window.nocksterReplies ??= {};
            window.nocksterReplies[$id] = { pending: true };
            window.nocksterNative.dispatch($request).then(value => {
                if (Object.hasOwn(window.nocksterReplies, $id)) window.nocksterReplies[$id] = { value };
            }).catch(() => {
                if (Object.hasOwn(window.nocksterReplies, $id)) window.nocksterReplies[$id] = { value: JSON.stringify({error:'Wallet engine failed'}) };
            });
            void 0;
        """.trimIndent())
        return try {
            withTimeout(180_000) {
                while (true) {
                    val reply = javascript("(() => { const entry = window.nocksterReplies[$id]; if (!entry || entry.pending) return null; delete window.nocksterReplies[$id]; return entry.value; })()")
                    if (reply is String) return@withTimeout JSONObject(reply)
                    delay(50)
                }
                @Suppress("UNREACHABLE_CODE") JSONObject()
            }
        } finally { withContext(NonCancellable) { javascript("delete window.nocksterReplies[$id]") } }
    }

    suspend fun perform(action: String, fields: JSONObject = JSONObject()): JSONObject? {
        if (busy || privateScreen) return null
        busy = true
        error = null
        val generation = session
        return try {
            val reply = call(action, fields)
            if (generation != session) null else {
                reply.optJSONObject("state")?.let { snapshot = it }
                if (!reply.isNull("error")) throw IllegalStateException(reply.getString("error"))
                reply
            }
        } catch (failure: Exception) {
            if (generation == session) error = failure.message
            null
        } finally { busy = false }
    }

    suspend fun updateVanityStatus() {
        val id = snapshot.optJSONObject("vanitySearch")?.text("id") ?: return
        if (busy || privateScreen) return
        val generation = session
        try {
            val reply = call("vanityStatus", JSONObject().put("searchId", id))
            if (generation == session && snapshot.optJSONObject("vanitySearch")?.text("id") == id) {
                snapshot = JSONObject(snapshot.toString()).put("vanitySearch", reply.optJSONObject("state")?.optJSONObject("vanitySearch") ?: JSONObject.NULL)
            }
        } catch (failure: CancellationException) { throw failure }
        catch (_: Exception) { /* Keep the search available for retry. */ }
    }

    suspend fun vanityStatus(searchId: String): JSONObject? {
        val generation = session
        return try {
            val reply = call("vanityStatus", JSONObject().put("searchId", searchId))
            if (generation == session && !privateScreen) reply.optJSONObject("vanity") else null
        } catch (failure: CancellationException) { throw failure }
        catch (failure: Exception) { JSONObject().put("status", "error").put("message", failure.message) }
    }

    suspend fun cancelVanity(searchId: String): JSONObject? {
        val generation = session
        return try {
            val reply = call("vanityStop", JSONObject().put("searchId", searchId))
            if (generation != session) null else {
                if (snapshot.optJSONObject("vanitySearch")?.text("id") == searchId) {
                    snapshot = JSONObject(snapshot.toString()).put("vanitySearch", reply.optJSONObject("state")?.optJSONObject("vanitySearch") ?: JSONObject.NULL)
                }
                reply.optJSONObject("vanity")
            }
        } catch (failure: CancellationException) { throw failure }
        catch (_: Exception) { null }
    }

    fun stopVanity(searchId: String) {
        scope.launch { cancelVanity(searchId) }
    }

    fun lock() {
        automaticUnlockPending = false
        session++
        snapshot = JSONObject().put("exists", snapshot.optBoolean("exists")).put("unlocked", false)
        error = null
        lockJob = scope.launch {
            if (ready) try { call("lock") } catch (failure: Exception) { error = "Unable to lock wallet. Close Nockster before continuing." }
        }
    }

    fun background(force: Boolean = false) {
        foreground = false
        if (force || (!authenticating && !cameraPermissionPrompt)) { privateScreen = true; lock() }
        if (force && !authenticating) automaticUnlockPending = true
    }
    fun foreground() {
        foreground = true
        scope.launch {
            lockJob?.join()
            if (!foreground) return@launch
            privateScreen = false
            automaticallyUnlock()
        }
    }

    private suspend fun automaticallyUnlock() {
        if (automaticUnlockPending && foreground && ready && deviceUnlockEnabled && snapshot.optBoolean("exists") &&
            !unlocked && !privateScreen && !busy && !authenticating) unlockWithDevice()
    }

    suspend fun enableDeviceUnlock(password: String) {
        if (busy || authenticating || privateScreen || !unlocked) return
        val key = perform("deviceUnlockKey", JSONObject().put("password", password))?.text("unlockKey") ?: return
        authenticating = true
        busy = true
        try {
            deviceUnlock.enable(key)
            deviceUnlockEnabled = true
            deviceUnlockWarning = null
        }
        catch (failure: Exception) {
            if (failure is HardwareUnlockUnavailable) deviceUnlockWarning = failure.message
            error = failure.message
        }
        finally { authenticating = false; busy = false }
    }

    suspend fun unlockWithDevice() {
        if (!foreground || !ready || !deviceUnlockEnabled || !snapshot.optBoolean("exists") || unlocked || busy || authenticating) return
        automaticUnlockPending = false
        authenticating = true
        busy = true
        val generation = session
        try {
            val key = deviceUnlock.secret()
            withTimeout(2_000) { while (!foreground) delay(50) }
            lockJob?.join()
            if (!foreground || generation != session) return
            privateScreen = false
            busy = false
            perform("unlockWithDeviceKey", JSONObject().put("key", key))
        } catch (failure: Exception) { error = failure.message }
        finally { authenticating = false; busy = false }
    }

    suspend fun confirmSubmission(action: String, fields: JSONObject, password: String?): JSONObject? {
        if (busy || privateScreen || !unlocked) return null
        if (password != null) return perform(action, JSONObject(fields.toString()).put("password", password))
        val generation = session
        authenticating = true
        busy = true
        return try {
            val key = deviceUnlock.secret("Confirm payment")
            withTimeout(2_000) { while (!foreground) delay(50) }
            check(generation == session && unlocked) { "Unlock the wallet and review the payment again" }
            busy = false
            perform(action, JSONObject(fields.toString()).put("key", key))
        } catch (failure: Exception) {
            error = failure.message
            null
        } finally { authenticating = false; busy = false }
    }

    fun receive(uri: Uri?) {
        if (uri == null || uri.scheme !in listOf("nockster", "web+nockster") || uri.host !in listOf("send", "pay") ||
            uri.userInfo != null || uri.port != -1 || uri.path !in listOf("", "/", null)) return
        val addresses = uri.getQueryParameters("to")
        val amounts = uri.getQueryParameters("amount")
        if (addresses.isEmpty() || addresses.size != amounts.size || addresses.size > 16) return
        pendingRecipients = addresses.zip(amounts)
    }
}
