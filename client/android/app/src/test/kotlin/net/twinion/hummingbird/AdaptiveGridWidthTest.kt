package net.twinion.hummingbird

import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import androidx.compose.ui.unit.width
import net.twinion.hummingbird.ui.LocalWideWindow
import net.twinion.hummingbird.ui.adaptiveGridCells
import net.twinion.hummingbird.ui.isWideWindow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** The unfolded slice's measuring half (`ChoiceRowWrappingTest`'s rig:
 * `@GraphicsMode(NATIVE)`, a width qualifier, and a control case that
 * proves the qualifier has teeth). `WindowWidthStructuralTest` pins the
 * wiring; this file renders the real `adaptiveGridCells()` at both sides
 * of the breakpoint and measures where things land — structural bounds
 * only, never `captureToImage`, which times out under Robolectric.
 *
 * The default qualifier is the wide side (840dp, past the 640 breakpoint);
 * the control method narrows to 320dp per-method. `sdk = [35]` is the
 * module's own minSdk, and the stock `Application` keeps `HummingbirdApp`'s
 * WorkManager lane out of a layout measurement. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(
    sdk = [35],
    qualifiers = "w840dp-h800dp",
    application = android.app.Application::class,
)
class AdaptiveGridWidthTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    /** The screens' own shape, miniature: a span-all header above item
     * cells, the grid taking `adaptiveGridCells()` under the provided
     * width answer — exactly what Triage/Done/Ledger/Alerts compose. */
    @Composable
    private fun SampleGrid() {
        CompositionLocalProvider(LocalWideWindow provides isWideWindow()) {
            LazyVerticalGrid(columns = adaptiveGridCells()) {
                item(key = "header", span = { GridItemSpan(maxLineSpan) }) {
                    Text("header", modifier = Modifier.fillMaxWidth().height(24.dp))
                }
                item(key = "one") { Text("card-one", modifier = Modifier.fillMaxWidth().height(48.dp)) }
                item(key = "two") { Text("card-two", modifier = Modifier.fillMaxWidth().height(48.dp)) }
            }
        }
    }

    @Test
    fun `two cells share a line on the wide side, under a full-width header`() {
        composeTestRule.setContent { SampleGrid() }

        val one = composeTestRule.onNodeWithText("card-one").getUnclippedBoundsInRoot()
        val two = composeTestRule.onNodeWithText("card-two").getUnclippedBoundsInRoot()
        val header = composeTestRule.onNodeWithText("header").getUnclippedBoundsInRoot()

        // 840dp affords two 320dp-minimum columns; the two cells share a
        // y-band instead of stacking.
        assertEquals(
            "the two cells must sit on one line at 840dp (one at y=${one.top}, two at y=${two.top})",
            one.top,
            two.top,
        )
        assertTrue(
            "the second cell must sit in its own column, to the right of the first " +
                "(one ends at ${one.right}, two starts at ${two.left})",
            two.left >= one.right,
        )
        assertTrue(
            "the span-all header must be wider than either cell " +
                "(header ${header.width}, cell ${one.width})",
            header.width > one.width && header.width > two.width,
        )
    }

    @Test
    @Config(qualifiers = "w320dp-h800dp")
    fun `the control - one fixed column on the phone side, cells stack`() {
        // Without this the wide test above could pass with the qualifier
        // ignored and the grid measuring at some default width — the same
        // reason ChoiceRowWrappingTest renders the defect on purpose.
        composeTestRule.setContent { SampleGrid() }

        val one = composeTestRule.onNodeWithText("card-one").getUnclippedBoundsInRoot()
        val two = composeTestRule.onNodeWithText("card-two").getUnclippedBoundsInRoot()

        assertTrue(
            "at 320dp the cells must stack (one at y=${one.top}, two at y=${two.top})",
            two.top >= one.top + one.height,
        )
        assertEquals(
            "and each must take the full single column (one ${one.width}, two ${two.width})",
            one.width,
            two.width,
        )
    }

    @Test
    fun `isWideWindow answers true past the breakpoint`() {
        var wide: Boolean? = null
        composeTestRule.setContent { wide = isWideWindow() }
        assertEquals(true, wide)
    }

    @Test
    @Config(qualifiers = "w320dp-h800dp")
    fun `isWideWindow answers false inside it`() {
        var wide: Boolean? = null
        composeTestRule.setContent { wide = isWideWindow() }
        assertEquals(false, wide)
    }
}
