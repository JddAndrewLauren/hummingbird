package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertHeightIsAtLeast
import androidx.compose.ui.test.assertWidthIsAtLeast
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onRoot
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.width
import net.twinion.hummingbird.ui.panes.QuietChip
import net.twinion.hummingbird.ui.theme.HummingbirdTheme
import org.junit.Assert.assertTrue
import uniffi.hummingbird_ffi_mobile.MobileKimiGap
import uniffi.hummingbird_ffi_mobile.MobileKimiResolved
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneBand
import uniffi.hummingbird_ffi_mobile.MobilePaneFacts
import uniffi.hummingbird_ffi_mobile.MobileProbeGap
import uniffi.hummingbird_ffi_mobile.MobileProbeResolved
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.MobileWorkflowGap
import uniffi.hummingbird_ffi_mobile.MobileWorkflowResolved
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** The quiet card's chip row (#689), measured — `ChoiceRowWrappingTest`'s
 * discipline applied to the one new touch target this screen introduces.
 *
 * The rule it guards: a chip is **exactly 44dp**, the house minimum
 * (`PaneRow`'s own `heightIn(min = 44.dp)`), and the row *wraps* rather
 * than squeezing when there are more chips than fit. Six is the real
 * number this device ranks today (`client/android/README.md`'s check 22,
 * measured 2026-08-19: reachability, Uptime x3, Kimi, GitHub), and the
 * width is the Fold's cover display — the operator's ruling that a
 * synthetic 320dp is not the screen this app is read on.
 *
 * **`@GraphicsMode(NATIVE)` and the `HummingbirdTheme` wrapper are both
 * load-bearing.** Legacy graphics measures text with a stub, and a bare
 * render resolves Material's default faces instead of the app's — either
 * one makes the numbers below fiction. There is deliberately **no
 * `captureToImage`**: it times out under this setup, and per the operator's
 * settled decision a Robolectric render is not visual evidence for this
 * surface anyway. This is a regression gate; the hardware run is the
 * evidence.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(
    sdk = [35],
    qualifiers = "w443dp-h960dp",
    application = android.app.Application::class,
)
@OptIn(ExperimentalLayoutApi::class)
class StatusChipRowTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    /** The six panes this device actually ranks, by their `paneLabel`
     * words — the chip's accessible name is the pane's label, since an
     * icon-only target with no name is a blank box to TalkBack. */
    private val sixPanes = listOf(
        MobileStandingQuestion.REACHABILITY to "Device reachability",
        MobileStandingQuestion.UPTIME to "Uptime — authority",
        MobileStandingQuestion.UPTIME to "Uptime — web",
        MobileStandingQuestion.UPTIME to "Uptime — runner",
        MobileStandingQuestion.KIMI to "Model credit balance",
        MobileStandingQuestion.GITHUB to "GitHub workflow — gmail-poll.yml",
    )

    private val labels = sixPanes.map { it.second }

    /** **Six chips do not wrap at 443dp** — 6x44dp plus five 8dp gaps is
     * 304dp, well inside the cover display — which the negative control
     * below proved the first time it was run against six. So the wrapping
     * rule is measured against the population that does need it: the ten
     * panes the web board already ranks (one Kimi, five GitHub workflows,
     * three uptime probes, one reachability), which is what this device
     * shows once every poller here is wired. Ten chips need 512dp. */
    private val tenPanes = sixPanes + listOf(
        MobileStandingQuestion.GITHUB to "GitHub workflow — calendar-poll.yml",
        MobileStandingQuestion.GITHUB to "GitHub workflow — graph-mail-poll.yml",
        MobileStandingQuestion.GITHUB to "GitHub workflow — graph-calendar-poll.yml",
        MobileStandingQuestion.GITHUB to "GitHub workflow — race-alert-poll.yml",
    )

    private fun pane(question: MobileStandingQuestion, label: String) = MobileRankedPane(
        standingQuestion = question,
        subjectKey = label,
        paneKey = "${question.name.lowercase()}:$label",
        answer = MobilePaneAnswer(
            answerState = MobilePaneAnswerState.ANSWERED,
            band = MobilePaneBand.DORMANT,
            withinBand = null,
        ),
        facts = quietFacts(question),
    )

    /** The chips are drawn from `standingQuestion` alone, so the simplest
     * honest facts arm per question is enough here — the facts themselves
     * are `StatusPanesExpanded`'s business, not the chip's. */
    private fun quietFacts(question: MobileStandingQuestion): MobilePaneFacts = when (question) {
        MobileStandingQuestion.REACHABILITY -> MobilePaneFacts.Reachability(facts = null)
        MobileStandingQuestion.UPTIME ->
            MobilePaneFacts.Uptime(resolved = MobileProbeResolved.Gap(MobileProbeGap.NotFetched))
        MobileStandingQuestion.KIMI ->
            MobilePaneFacts.Kimi(resolved = MobileKimiResolved.Gap(MobileKimiGap.NotFetched))
        MobileStandingQuestion.GITHUB ->
            MobilePaneFacts.Github(
                resolved = MobileWorkflowResolved.Gap(MobileWorkflowGap.NotFetched),
            )
        MobileStandingQuestion.HOMEWORK,
        MobileStandingQuestion.WASTE,
        MobileStandingQuestion.WEEKEND,
        MobileStandingQuestion.VACATION,
        MobileStandingQuestion.RACE,
        -> error("a Now-surface question has no chip on Status: $question")
    }

    @Composable
    private fun Chips(
        panes: List<Pair<MobileStandingQuestion, String>>,
        wrapping: Boolean,
    ) {
        HummingbirdTheme {
            if (wrapping) {
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    for ((question, label) in panes) {
                        QuietChip(pane(question, label), label, selected = false, onToggle = {})
                    }
                }
            } else {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for ((question, label) in panes) {
                        QuietChip(pane(question, label), label, selected = false, onToggle = {})
                    }
                }
            }
        }
    }

    @Test
    fun `every chip keeps its 44dp target at the Fold's cover width`() {
        composeTestRule.setContent { Chips(sixPanes, wrapping = true) }

        for (label in labels) {
            composeTestRule.onNodeWithContentDescription(label).assertWidthIsAtLeast(44.dp)
            composeTestRule.onNodeWithContentDescription(label).assertHeightIsAtLeast(44.dp)
        }
    }

    @Test
    fun `the row wraps rather than squeezing, so every chip keeps its target`() {
        composeTestRule.setContent { Chips(tenPanes, wrapping = true) }

        // Not "the last chip stays inside the edge": a `Row` *clips* rather
        // than overflowing, so that assertion is true of the broken layout
        // too (it was, until a mutation run caught it). What actually
        // separates wrapping from not wrapping is that every chip keeps its
        // full 44dp — the control below shows the `Row` losing it.
        for ((_, label) in tenPanes) {
            composeTestRule.onNodeWithContentDescription(label).assertWidthIsAtLeast(44.dp)
        }
    }

    /** The control that gives the assertion above its teeth: the same ten
     * chips in a non-wrapping `Row` do not overflow the edge — the `Row`
     * constrains them, so the trailing chips are **squeezed below the 44dp
     * touch target** instead, which is the shape the defect really takes
     * (`ChoiceRowWrappingTest` measured the same squeeze as a zero-width
     * button). Without this, a widened qualifier or a shrunken chip would
     * leave the wrap test green while measuring nothing. */
    @Test
    fun `a plain Row squeezes its trailing chips below the touch target`() {
        composeTestRule.setContent { Chips(tenPanes, wrapping = false) }

        val last = composeTestRule
            .onNodeWithContentDescription(tenPanes.last().second)
            .getUnclippedBoundsInRoot()
        assertTrue(
            "an unwrapped row must squeeze its last chip under 44dp, " +
                "or this file measures nothing (width=${last.width})",
            last.width < 44.dp,
        )
    }
}
