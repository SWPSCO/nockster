package io.swps.nockster

import android.content.Intent
import android.app.Activity
import android.content.Context
import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.FrameLayout
import androidx.core.view.WindowCompat
import androidx.compose.ui.platform.ComposeView
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.lifecycle.lifecycleScope
import com.getcapacitor.BridgeActivity
import kotlinx.coroutines.launch

class MainActivity : BridgeActivity() {
    private var model: WalletModel? = null
    private var screenCapture: ScreenCaptureSettings? = null
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val capture = ScreenCaptureSettings(this)
        screenCapture = capture
        capture.apply()
        WindowCompat.setDecorFitsSystemWindows(window, false)
        val webView = bridge.webView
        webView.visibility = View.INVISIBLE
        webView.setBackgroundColor(android.graphics.Color.TRANSPARENT)
        webView.isFocusable = false
        webView.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
        val wallet = WalletModel(webView, DeviceUnlock(this), lifecycleScope, capture)
        model = wallet
        wallet.receive(intent?.data)
        // Keep the engine attached without a full-screen WebView layer behind
        // the native UI. Compose owns the window and its keyboard insets.
        (webView.parent as? ViewGroup)?.removeView(webView)
        setContentView(FrameLayout(this).apply {
            addView(webView, FrameLayout.LayoutParams(1, 1))
            addView(ComposeView(this@MainActivity).apply { setContent { NocksterApp(wallet) } },
                FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        })
        lifecycleScope.launch { wallet.start() }
    }
    override fun onPause() {
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        model?.background()
        super.onPause()
    }
    override fun onStop() { model?.background(force = true); super.onStop() }
    override fun onResume() { super.onResume(); screenCapture?.apply(); model?.foreground() }
    override fun onNewIntent(intent: Intent) { super.onNewIntent(intent); model?.receive(intent.data) }
}

class ScreenCaptureSettings(private val activity: Activity) {
    private val preferences = activity.getSharedPreferences("privacy", Context.MODE_PRIVATE)
    var allowed by mutableStateOf(preferences.getBoolean("allow-screenshots", true))
        private set

    fun updateAllowed(value: Boolean) {
        preferences.edit().putBoolean("allow-screenshots", value).apply()
        allowed = value
        apply()
    }

    fun apply() {
        if (allowed) activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        else activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    }
}
