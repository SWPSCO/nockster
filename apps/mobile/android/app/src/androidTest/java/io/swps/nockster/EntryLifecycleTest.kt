package io.swps.nockster

import android.view.WindowManager
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.Lifecycle
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class EntryLifecycleTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()
    private val model: WalletModel
        get() = MainActivity::class.java.getDeclaredField("model").apply { isAccessible = true }.get(ui.activity) as WalletModel

    @Before fun ready() {
        ui.waitUntil(20_000) { model.ready }
        ui.runOnIdle {
            model.snapshot = JSONObject().put("exists", false).put("unlocked", false).put("suggestedWalletName", "My Wallet")
        }
    }

    private fun switchApps() {
        ui.activityRule.scenario.moveToState(Lifecycle.State.CREATED)
        ui.activityRule.scenario.onActivity { activity ->
            assertTrue("The OS protects the background window", activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
            assertFalse("The wallet locks on background", model.unlocked)
        }
        ui.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
        ui.waitUntil(10_000) { !model.privateScreen }
        ui.runOnIdle {
            assertEquals("Screenshot preference applies on return", !model.screenCapture.allowed,
                ui.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
        }
    }

    @Test fun importDraftSurvivesAppSwitching() {
        ui.onNodeWithText("Import", useUnmergedTree = true).performClick()
        ui.onNodeWithText("Wallet name").performTextReplacement("Travel")
        ui.onNodeWithText("Recovery phrase or extended private key").performTextInput("synthetic unfinished import")
        ui.onNodeWithText("Wallet password").performScrollTo().performTextInput("synthetic setup password")
        ui.onNodeWithText("Confirm password").performScrollTo().performTextInput("synthetic setup password")
        switchApps()
        ui.onNodeWithText("Wallet name").assertTextContains("Travel")
        ui.onNodeWithText("Recovery phrase or extended private key").assertTextContains("synthetic unfinished import")
        ui.onNodeWithText("Wallet password").assertTextContains("synthetic setup password")
        ui.onNodeWithText("Confirm password").assertTextContains("synthetic setup password")
        switchApps()
        ui.onNodeWithText("Recovery phrase or extended private key").assertTextContains("synthetic unfinished import")
    }

    @Test fun recoveryPhraseSurvivesAppSwitching() {
        ui.onNodeWithText("Generate Recovery Phrase").performClick()
        ui.waitUntil(10_000) { ui.onAllNodesWithText("Secret Recovery Phrase").fetchSemanticsNodes().isNotEmpty() }
        val phrase = ui.onAllNodes(hasText(". ", substring = true)).fetchSemanticsNodes()
            .flatMap { it.config[androidx.compose.ui.semantics.SemanticsProperties.Text] }.map { it.text }
            .filter { it.matches(Regex("\\d+\\. [a-z]+")) }
        assertEquals(24, phrase.size)
        switchApps()
        phrase.forEach { ui.onNodeWithText(it).assertExists() }
    }

    @Test fun lockedPasswordEntrySurvivesAppSwitching() {
        ui.runOnIdle { model.snapshot = JSONObject().put("exists", true).put("unlocked", false) }
        ui.onNodeWithText("Wallet password").performTextInput("synthetic unlock password")
        switchApps()
        ui.onNodeWithText("Wallet password").assertTextContains("synthetic unlock password")
        ui.onNodeWithText("Unlock Wallet").assertExists()
        ui.runOnIdle { assertFalse(model.unlocked) }
    }
}
