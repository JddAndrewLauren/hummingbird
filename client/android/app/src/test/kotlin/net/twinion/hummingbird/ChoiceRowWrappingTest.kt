package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.ui.test.assertWidthIsAtLeast
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import androidx.compose.ui.unit.width
import java.io.File
import net.twinion.hummingbird.ui.ChoiceRow
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** #576's regression gate — and the first test in `client/android` that
 * measures a layout at all.
 *
 * Every other no-emulator gate here (`NowScreenStructuralTest`,
 * `ColorTokenDriftTest`, `ManifestAliasTest`) asserts presence and wiring,
 * which is exactly why a row that squeezed its trailing button into a
 * one-character-per-line column shipped to four screens: the buttons were
 * all present and all wired, and nothing measured them. So this file does
 * two things a structural assertion cannot.
 *
 * **It measures.** `createComposeRule()` under Robolectric at a 320dp-wide
 * qualifier renders the real `ChoiceRow` and asserts every choice is at
 * least 48dp wide (Material's minimum touch target) and no taller than one
 * line of button.
 *
 * **It proves the measurement has teeth.** `a plain Row is what the defect
 * looked like` renders the same four labels in the `Row` the sites used to
 * have and asserts the trailing one *is* squeezed — it measures 0dp wide
 * and 136dp tall, which is the vertical letter column, in numbers. Without
 * that control a widened qualifier, a shortened label set or a regression
 * in Robolectric's text measurement would leave this file green while
 * measuring nothing, which is the failure mode a width-dependent test is
 * most exposed to.
 *
 * **`@GraphicsMode(NATIVE)` is load-bearing, not decoration.** In
 * Robolectric's default (legacy) graphics mode text is measured by a stub
 * that returns near-identical widths for every string: the same four
 * buttons come back 58dp wide in a `Row` and in a `FlowRow` alike, the
 * defect does not reproduce, and this file would assert nothing. Native
 * graphics puts real Minikin measurement behind the render, which is what
 * makes the numbers above real. Do not drop it to make something else pass.
 *
 * The per-site half is `the four sites reach for ChoiceRow, not a bare Row`
 * below: measuring the shared component proves the component wraps, and
 * that source pin — the same "parse the real source" discipline
 * `NowScreenStructuralTest` uses — is what proves the four sites use it.
 *
 * `sdk = [35]` is this module's own `minSdk`, and the stock `Application`
 * keeps `HummingbirdApp`'s WorkManager lane out of a layout measurement.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(
    sdk = [35],
    qualifiers = "w320dp-h800dp",
    application = android.app.Application::class,
)
class ChoiceRowWrappingTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    // `ItemDetailScreen`'s own four, worded as `ACTION_LABEL` words
    // `decisions::actions::available_actions` — the set that failed on
    // hardware, where `Cancel` was the one that went vertical.
    private val fourActions = listOf("Start", "Complete", "Mark blocked", "Cancel")

    // A button holding one line of its own label. The real render is 40dp;
    // the margin absorbs font-metric drift without admitting the 96dp and
    // 136dp two- and six-line columns the defect produced.
    private val oneLine = 56.dp

    @Test
    fun `all four item actions stay hittable at 320dp`() {
        composeTestRule.setContent {
            ChoiceRow {
                for (action in fourActions) {
                    OutlinedButton(onClick = {}) { Text(action) }
                }
            }
        }

        for (action in fourActions) {
            composeTestRule.onNodeWithText(action).assertWidthIsAtLeast(48.dp)
            assertOneLineTall(action)
        }
    }

    @Test
    fun `a full-sentence grill choice stays hittable at 320dp`() {
        // Site 2's real shape: `grill-me` returns sentences, not words, and
        // two of them do not share a line on any phone. Wrapping is the
        // whole answer — each choice gets a line of its own.
        val choices = listOf(
            "Yes, list it privately first",
            "No, trade it in at the dealer",
        )
        composeTestRule.setContent {
            ChoiceRow {
                for (choice in choices) {
                    OutlinedButton(onClick = {}) { Text(choice) }
                }
            }
        }

        for (choice in choices) {
            composeTestRule.onNodeWithText(choice).assertWidthIsAtLeast(48.dp)
            assertOneLineTall(choice)
        }
    }

    @Test
    fun `a plain Row is what the defect looked like`() {
        composeTestRule.setContent {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (action in fourActions) {
                    OutlinedButton(onClick = {}) { Text(action) }
                }
            }
        }

        val cancel = composeTestRule.onNodeWithText("Cancel").getUnclippedBoundsInRoot()
        assertTrue(
            "a plain Row at 320dp must still squeeze the trailing action below the " +
                "48dp touch target and stretch it past one line — otherwise this " +
                "file's other tests would pass with or without the fix " +
                "(measured ${cancel.width} x ${cancel.height})",
            cancel.width < 48.dp && cancel.height > oneLine,
        )
    }

    @Test
    fun `the four sites reach for ChoiceRow, not a bare Row`() {
        val sites = listOf(
            // Site 1: the item's own action row (`Cancel` was vertical).
            "ItemDetailScreen.kt",
            // Sites 2 and 3: the interview's answer chips, and the `Keep`
            // that escapes the discard prompt.
            "GrillTakeoverScreen.kt",
            // Site 4: `Grill me` + `Promote to ready` — same shape, never
            // sighted failing, fixed with the rest so it cannot start.
            "TriageScreen.kt",
        )
        for (file in sites) {
            val src = repoFile(
                "client/android/app/src/main/kotlin/net/twinion/hummingbird/$file",
            )
            assertTrue(
                "$file must lay its choices out with ChoiceRow (#576)",
                src.contains("ChoiceRow"),
            )
        }
    }

    // Compose ships `assertHeightIsAtLeast`, not an at-most — the bound
    // that matters here is the upper one, since the defect's signature is a
    // label standing up into a column, so it is measured by hand.
    private fun assertOneLineTall(label: String) {
        val bounds = composeTestRule.onNodeWithText(label).getUnclippedBoundsInRoot()
        assertTrue(
            "\"$label\" must sit on one line, not stand up as a letter column " +
                "(measured ${bounds.width} x ${bounds.height})",
            bounds.height <= oneLine,
        )
    }

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }
}
