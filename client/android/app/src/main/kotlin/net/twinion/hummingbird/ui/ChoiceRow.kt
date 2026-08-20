package net.twinion.hummingbird.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.FlowRowScope
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** One row of buttons or chips the user picks from — laid out so that a
 * choice too wide for the display moves to the next line instead of being
 * squeezed.
 *
 * **This exists because the same defect shipped four times (#576).** A plain
 * `Row` does not clip its overflow: it hands the trailing child whatever
 * width is left, which for a fourth button on a phone is a few dp, and the
 * label then wraps one character per line — a vertical column of letters
 * that still consumes layout height and pushes everything below it off the
 * screen. On `GrillTakeoverScreen`'s discard prompt that trailing child was
 * `Keep`, the escape from a destructive question, so the failure is not
 * always cosmetic.
 *
 * `NowScreen.kt`, `RulesScreen.kt` and `PriorityRow.kt` each answered this
 * with their own inline `FlowRow` before this file existed; the answer was
 * the right one and is simply named here, so the fifth site inherits it
 * rather than rediscovering it. Those three were deliberately left as they
 * were — they are not broken, and rewriting working screens is not what
 * #576 asked for. `PriorityRow` has since left the group: it dropped its
 * fifth chip on operator decision 2026-08-20, and four fit one line at the
 * narrowest width the app ships to, so it is a fixed `Row` now with
 * `PriorityRowWrappingTest` measuring that it stays one.
 *
 * The 8.dp on both axes is the spacing every one of those precedents
 * already uses (`Arrangement.spacedBy(8.dp)`), and it is on the design
 * system's scale — `--space-4` in `hummingbird-design/tokens/spacing.css`,
 * the same step the row already used horizontally. It is a token decision,
 * not a per-site one, which is the other reason it lives in one place now.
 *
 * `ChoiceRowWrappingTest` is what holds this: it measures a real render at
 * 320dp, and asserts against a plain `Row` in the same test that the
 * measurement has teeth.
 */
@Composable
internal fun ChoiceRow(
    modifier: Modifier = Modifier,
    content: @Composable FlowRowScope.() -> Unit,
) {
    FlowRow(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        content = content,
    )
}
