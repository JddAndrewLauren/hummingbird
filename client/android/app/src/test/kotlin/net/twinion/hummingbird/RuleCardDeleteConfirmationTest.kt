package net.twinion.hummingbird

import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.hummingbird_ffi_mobile.MobileTier
import uniffi.hummingbird_ffi_mobile.RuleRecord

// #728: `RuleCard`'s two-step delete (tap `Delete` → confirm in the
// `AlertDialog`) had no test proving the two steps are real rather than
// cosmetic. `RulesViewModelTest` drives `RulesViewModel.delete` directly
// and thoroughly, but nothing exercised the card's own gate — a wiring
// change that called `onDelete` straight from the row's `Delete` button
// would have passed every existing test in this module.
//
// This is a Robolectric Compose interaction test, not a source-text one
// (`RulesScreenStructuralTest`'s discipline is the wrong tool here — a
// grep for `AlertDialog` cannot prove a tap sends nothing). `RuleCard` was
// widened from `private` to `internal` so this file can render it
// directly with a fake `onDelete`, the same shape `RulesViewModelTest`
// already uses for the seam calls the ViewModel makes.
//
// Each assertion here was checked against a mutated `RuleCard` that wired
// the first tap straight to `onDelete()` (skipping `confirmingDelete`
// entirely) — all three tests below fail against that mutation, which
// is what "not gated by nothing" now means.
//
// This is the module's only Compose-rendering Robolectric test without
// `@GraphicsMode(GraphicsMode.Mode.NATIVE)`, deliberately: unlike its
// siblings (`AxisRowWrappingTest`, `AdaptiveGridWidthTest`,
// `DeadlineFieldWrappingTest`, `ChoiceRowWrappingTest`), where NATIVE mode
// is load-bearing because they measure layout, nothing here asserts a
// dimension — only which nodes exist and what gets called.
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = android.app.Application::class)
class RuleCardDeleteConfirmationTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    private fun rule(id: String = "r-1") = RuleRecord(
        id = id,
        name = "passport",
        eventKind = "email",
        kindLabelKey = "email",
        conditions = emptyList(),
        severity = "high",
        tier = MobileTier.URGENT,
        enabled = true,
        isValid = true,
        invalidFields = emptyList(),
        severityIsUnranked = false,
        version = 3,
    )

    private fun render(onDelete: () -> Unit) {
        composeTestRule.setContent {
            RuleCard(
                rule = rule(),
                pending = null,
                pendingDeleted = false,
                onEdit = {},
                onSetEnabled = {},
                onDelete = onDelete,
            )
        }
    }

    @Test
    fun `the first tap on Delete sends nothing`() {
        var deleteCalls = 0
        render(onDelete = { deleteCalls++ })

        composeTestRule.onNodeWithText("Delete").performClick()

        assertEquals(
            "tapping the row's Delete button must not invoke onDelete on its own",
            0,
            deleteCalls,
        )
        // And it opened the confirmation rather than doing nothing at all.
        composeTestRule.onNodeWithText("Delete this rule?").assertExists()
    }

    @Test
    fun `only the dialog's own confirm button invokes onDelete`() {
        var deleteCalls = 0
        render(onDelete = { deleteCalls++ })

        composeTestRule.onNodeWithText("Delete").performClick()
        composeTestRule
            .onNode(hasText("Delete") and hasAnyAncestor(isDialog()))
            .performClick()

        assertEquals(
            "the dialog's confirm button is the only thing that may call onDelete",
            1,
            deleteCalls,
        )
    }

    @Test
    fun `cancelling sends nothing and leaves the card interactive`() {
        var deleteCalls = 0
        render(onDelete = { deleteCalls++ })

        composeTestRule.onNodeWithText("Delete").performClick()
        composeTestRule.onNodeWithText("Cancel").performClick()

        assertEquals(0, deleteCalls)
        // The dialog is gone...
        composeTestRule.onNodeWithText("Delete this rule?").assertDoesNotExist()
        // ...and the card is interactive again: a second tap still only
        // opens the dialog, rather than the row being left in some stuck
        // state that swallows or short-circuits the next tap.
        composeTestRule.onNodeWithText("Delete").performClick()
        composeTestRule.onNodeWithText("Delete this rule?").assertExists()
        assertEquals(0, deleteCalls)
    }
}
