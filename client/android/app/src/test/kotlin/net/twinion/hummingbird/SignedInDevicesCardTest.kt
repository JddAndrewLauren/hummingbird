package net.twinion.hummingbird

import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import net.twinion.hummingbird.ui.theme.HummingbirdTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.hummingbird_ffi_mobile.MobileDeviceRow

/** Settings' Signed-in devices section (#841), composed with fakes: the
 * rows draw in the order handed over (the core's — `SettingsViewModelTest`
 * holds that this device leads it), a row the authority will not revoke
 * draws no sign-out control, and every sign-out asks before it acts.
 *
 * Structure, not pixels — `SettingsQuestionRowsTest`'s own note. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = android.app.Application::class)
class SignedInDevicesCardTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    private fun row(id: String, isCurrent: Boolean, revocable: Boolean) = MobileDeviceRow(
        id = id,
        name = "$id name",
        isCurrent = isCurrent,
        thisDevice = if (isCurrent) "This device" else null,
        platformLabel = "Android",
        lastSeenLabel = "Last seen just now",
        revocable = revocable,
    )

    private val rows = listOf(
        row("device-android-pixel", isCurrent = true, revocable = true),
        row("runner", isCurrent = false, revocable = false),
        row("device-web-laptop", isCurrent = false, revocable = true),
    )

    private val signedOut = mutableListOf<String>()

    private fun compose() {
        composeTestRule.setContent {
            HummingbirdTheme {
                SignedInDevicesCard(
                    state = DevicesState.Listed(rows),
                    error = null,
                    confirmation = { "Confirm ${it.id}?" },
                    onSignOut = { signedOut += it.id },
                )
            }
        }
    }

    @Test
    fun `a row that cannot be revoked draws no sign-out control`() {
        compose()

        composeTestRule.onNodeWithText("runner name").assertExists()
        composeTestRule.onAllNodesWithText("Sign out of this device").assertCountEquals(1)
        composeTestRule.onAllNodesWithText("Sign out").assertCountEquals(1)
    }

    @Test
    fun `this device's row is drawn first and labelled`() {
        compose()

        val tops = rows.map { composeTestRule.onNodeWithText(it.name).fetchSemanticsNode().boundsInRoot.top }
        assertEquals("rows draw in the order handed over", tops.sorted(), tops)
        composeTestRule.onNodeWithText("This device · Android · Last seen just now").assertExists()
    }

    @Test
    fun `signing out asks first, and only the confirmation acts`() {
        compose()

        composeTestRule.onNodeWithText("Sign out of this device").performClick()
        composeTestRule.onNodeWithText("Confirm device-android-pixel?").assertExists()
        assertTrue("nothing happens before the confirmation", signedOut.isEmpty())

        composeTestRule.onNodeWithText("Cancel").performClick()
        assertTrue(signedOut.isEmpty())

        composeTestRule.onNodeWithText("Sign out of this device").performClick()
        composeTestRule.onNode(hasText("Sign out") and hasAnyAncestor(isDialog())).performClick()
        assertEquals(listOf("device-android-pixel"), signedOut)
    }
}
