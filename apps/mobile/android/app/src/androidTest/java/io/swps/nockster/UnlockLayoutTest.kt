package io.swps.nockster

import android.graphics.Bitmap
import android.os.ParcelFileDescriptor
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

@RunWith(AndroidJUnit4::class)
class UnlockLayoutTest {
    @Test fun captureLockedScreenWithKeyboard() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val automation = instrumentation.uiAutomation
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            fun search(node: android.view.accessibility.AccessibilityNodeInfo?, text: String): android.view.accessibility.AccessibilityNodeInfo? {
                if (node == null) return null
                if (node.text?.toString() == text) return node
                for (index in 0 until node.childCount) search(node.getChild(index), text)?.let { return it }
                return null
            }
            fun find(text: String) = search(automation.rootInActiveWindow, text)
            repeat(60) {
                if (find("Wallet password") == null) Thread.sleep(250)
            }
            assertNotNull("The saved vault opens on its unlock screen", find("Unlock Wallet"))
            fun capture(name: String) {
                val bitmap = automation.takeScreenshot()
                assertNotNull(bitmap)
                File(instrumentation.targetContext.getExternalFilesDir(null), name).outputStream().use {
                    bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)
                }
            }
            capture("unlock.png")
            val bounds = android.graphics.Rect()
            find("Wallet password")!!.getBoundsInScreen(bounds)
            fun shell(command: String) {
                ParcelFileDescriptor.AutoCloseInputStream(automation.executeShellCommand(command)).use { it.readBytes() }
            }
            shell("input tap ${bounds.centerX()} ${bounds.centerY()}")
            shell("input text synthetic-layout-password")
            Thread.sleep(1000)
            assertNotNull("The unlock action stays available with the keyboard open", find("Unlock Wallet"))
            capture("unlock-keyboard.png")
            find("Unlock Wallet")!!.parent.getBoundsInScreen(bounds)
            assertTrue("The entire unlock button fits above the keyboard", bounds.height() >= 48 * instrumentation.targetContext.resources.displayMetrics.density)
            shell("input keyevent 4")
        }
    }
}
