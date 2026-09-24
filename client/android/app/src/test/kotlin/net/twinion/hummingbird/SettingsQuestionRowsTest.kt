package net.twinion.hummingbird

import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import net.twinion.hummingbird.ui.theme.HummingbirdTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.hummingbird_ffi_mobile.MobileBindingRecord
import uniffi.hummingbird_ffi_mobile.MobileBindingValue
import uniffi.hummingbird_ffi_mobile.MobileQuestionRosterEntry
import uniffi.hummingbird_ffi_mobile.MobileQuestionSwitch
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.MobileSurface

/** Settings' `Standing questions` rows (#716), composed with fakes.
 *
 * The one behaviour here a source pin cannot hold is the `rememberSaveable`
 * identity trap: every row of a loop sits at the same composition position,
 * so an open state saved without the row's own key is restored onto the
 * wrong row — or onto every row — after a fold, a rotation or a process
 * death. [StateRestorationTester] runs exactly that save-and-restore.
 *
 * Structure, not pixels: `captureToImage()` never completes under this
 * Robolectric setup (`docs/SURFACES.md`); what the rows look like is the
 * operator's hardware run. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = android.app.Application::class)
class SettingsQuestionRowsTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    private val roster = listOf(
        MobileQuestionRosterEntry(MobileStandingQuestion.WASTE, "Third word", MobileSurface.NOW, listOf("key-w")),
        MobileQuestionRosterEntry(MobileStandingQuestion.HOMEWORK, "First word", MobileSurface.NOW, listOf("key-h")),
        MobileQuestionRosterEntry(MobileStandingQuestion.KIMI, "Second word", MobileSurface.STATUS, emptyList()),
    )

    private fun binding(key: String, known: Boolean = true) =
        MobileBindingRecord(key = key, known = known, pending = false, value = MobileBindingValue.Unset)

    private fun switches(off: Set<MobileStandingQuestion> = emptySet()) =
        roster.map { MobileQuestionSwitch(it.question, enabled = it.question !in off, pending = false) }

    private val toggles = mutableListOf<Pair<MobileStandingQuestion, Boolean>>()

    private fun card(grouped: GroupedBindings): @androidx.compose.runtime.Composable () -> Unit = {
        HummingbirdTheme {
            StandingQuestionsCard(
                grouped = grouped,
                bindingError = null,
                switchError = null,
                onSaveBinding = { _, _ -> },
                onSetEnabled = { question, enabled -> toggles += question to enabled },
            )
        }
    }

    @Test
    fun `rows do not share expansion state across a save and restore`() {
        val restoration = StateRestorationTester(composeTestRule)
        restoration.setContent(
            card(groupBindingsByQuestion(roster, listOf(binding("key-w"), binding("key-h")), switches())),
        )

        composeTestRule.onNodeWithText("First word").performClick()
        composeTestRule.onNodeWithText("key-h").assertExists()
        composeTestRule.onNodeWithText("key-w").assertDoesNotExist()

        restoration.emulateSavedInstanceStateRestore()

        // The opened row is still the only open one — not the first row,
        // not every row.
        composeTestRule.onNodeWithText("key-h").assertExists()
        composeTestRule.onNodeWithText("key-w").assertDoesNotExist()
        composeTestRule.onNodeWithContentDescription("Asked — First word").assertExists()
        composeTestRule.onNodeWithContentDescription("Asked — Third word").assertDoesNotExist()
        composeTestRule.onNodeWithContentDescription("Asked — Second word").assertDoesNotExist()
    }

    @Test
    fun `every row starts collapsed and an off question says so while shut`() {
        composeTestRule.setContent(
            card(groupBindingsByQuestion(roster, emptyList(), switches(off = setOf(MobileStandingQuestion.KIMI)))),
        )

        for (entry in roster) {
            composeTestRule.onNodeWithText(entry.label).assertExists()
            composeTestRule.onNodeWithContentDescription("Asked — ${entry.label}").assertDoesNotExist()
        }
        composeTestRule.onNodeWithText("off").assertExists()
    }

    @Test
    fun `the toggle hands its question and new state to the seam's write`() {
        composeTestRule.setContent(card(groupBindingsByQuestion(roster, emptyList(), switches())))

        composeTestRule.onNodeWithText("Second word").performClick()
        composeTestRule.onNodeWithContentDescription("Asked — Second word").performClick()

        assertEquals(listOf(MobileStandingQuestion.KIMI to false), toggles)
    }

    @Test
    fun `unknown settings rows still render under Other settings rows`() {
        composeTestRule.setContent(
            card(groupBindingsByQuestion(roster, listOf(binding("mystery-key", known = false)), switches())),
        )

        composeTestRule.onNodeWithText("Other settings rows").performClick()

        composeTestRule.onNodeWithText("mystery-key").assertExists()
    }

    @Test
    fun `an empty mirror renders every question and no Other settings rows group`() {
        composeTestRule.setContent(card(groupBindingsByQuestion(roster, emptyList(), switches())))

        for (entry in roster) {
            composeTestRule.onNodeWithText(entry.label).assertExists()
        }
        composeTestRule.onNodeWithText("Other settings rows").assertDoesNotExist()
        composeTestRule.onNodeWithText("Third word").performClick()
        composeTestRule.onNodeWithText("No settings row for key-w yet.").assertExists()
    }
}
