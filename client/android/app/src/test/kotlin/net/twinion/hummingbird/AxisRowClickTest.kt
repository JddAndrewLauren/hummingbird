package net.twinion.hummingbird

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import net.twinion.hummingbird.ui.theme.HummingbirdTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.hummingbird_ffi_mobile.MobileCalmOrder
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis

/** The Urgency chip's two gestures (operator decision 2026-09-13): the
 * calm-order arrow rides inside that chip, so tapping it while it is the
 * live axis flips the direction, and tapping it from any other axis picks
 * the axis. Nothing else distinguishes the two — a regression that routed
 * both taps to `onPick` would leave the arrow drawn and inert. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = android.app.Application::class)
class AxisRowClickTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    private val picked = mutableListOf<MobileFrontierAxis>()
    private var flips = 0

    private fun render(axis: MobileFrontierAxis, calmOrder: MobileCalmOrder = MobileCalmOrder.OLDEST) {
        composeTestRule.setContent {
            HummingbirdTheme {
                AxisRow(
                    axis = axis,
                    onPick = { picked += it },
                    calmOrder = calmOrder,
                    onFlipCalmOrder = { flips += 1 },
                    filtersOpen = false,
                    facetCount = 0,
                    onToggleFilters = {},
                )
            }
        }
    }

    @Test
    fun `tapping the live Urgency chip flips the order rather than re-picking the axis`() {
        render(MobileFrontierAxis.URGENCY)

        composeTestRule.onNodeWithText("Urgency").performClick()

        assertEquals(1, flips)
        assertEquals(emptyList<MobileFrontierAxis>(), picked)
    }

    @Test
    fun `tapping Urgency from another axis picks it and flips nothing`() {
        render(MobileFrontierAxis.CONTEXT)

        composeTestRule.onNodeWithText("Urgency").performClick()

        assertEquals(listOf(MobileFrontierAxis.URGENCY), picked)
        assertEquals(0, flips)
    }

    @Test
    fun `the live Urgency chip speaks its direction`() {
        render(MobileFrontierAxis.URGENCY, MobileCalmOrder.NEWEST)

        composeTestRule.onNode(
            SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "Newest first"),
        ).assert(SemanticsMatcher.expectValue(SemanticsProperties.Selected, true))
    }
}
