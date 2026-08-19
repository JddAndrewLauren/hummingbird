package net.twinion.hummingbird.ui

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import net.twinion.hummingbird.R
import net.twinion.hummingbird.ui.theme.Amber100
import net.twinion.hummingbird.ui.theme.Amber600
import net.twinion.hummingbird.ui.theme.Crimson100
import net.twinion.hummingbird.ui.theme.Crimson600
import net.twinion.hummingbird.ui.theme.Ember400
import net.twinion.hummingbird.ui.theme.Ember50
import net.twinion.hummingbird.ui.theme.Ember600
import net.twinion.hummingbird.ui.theme.Ink100
import net.twinion.hummingbird.ui.theme.Ink400
import net.twinion.hummingbird.ui.theme.Moss100
import net.twinion.hummingbird.ui.theme.Moss600
import net.twinion.hummingbird.ui.theme.Sky100
import net.twinion.hummingbird.ui.theme.Sky600
import net.twinion.hummingbird.ui.theme.StageBlockedBgDark
import net.twinion.hummingbird.ui.theme.StageBlockedFgDark
import net.twinion.hummingbird.ui.theme.StageDoneBgDark
import net.twinion.hummingbird.ui.theme.StageGrillingBgDark
import net.twinion.hummingbird.ui.theme.StageGrillingFgDark
import net.twinion.hummingbird.ui.theme.StageProgressBgDark
import net.twinion.hummingbird.ui.theme.StageReadyBgDark
import net.twinion.hummingbird.ui.theme.StageReadyFgDark
import net.twinion.hummingbird.ui.theme.StageTriageBgDark
import net.twinion.hummingbird.ui.theme.StageTriageFgDark

// The one way a stage renders (#557) — `client/web/src/components/domain/
// StageBadge.tsx`, ported: every screen that shows an item's stage goes
// through this composable, so the funnel word on a Now card, a Triage row,
// the Ledger and item detail is one word in one treatment. A coloured pill
// always encodes stage, tier or urgency (the design README's colour rule);
// stage colour comes from the `--stage-*` token pairs, hand-ported into
// `ui/theme/Color.kt` under `ColorTokenDriftTest` (ADR-0026).
//
// Three forms, chosen exactly as the web does: `compact` is an 8dp dot for
// dense rows; a stage with a glyph (only `triage` — the ICONOGRAPHY
// roster's `inbox`) draws the glyph and drops both the word *and* the dot,
// since a coloured glyph already carries what the dot exists to carry; every
// other stage keeps the dot-and-word pill. The word-free pill names itself
// through `Role.Image` + `contentDescription` — the Compose port of the
// web's `role="img"`/`aria-label` contract; the web's `title` tooltip has
// no Compose analogue at this size, a recorded divergence, and the
// accessible name is the part that was load-bearing.
//
// "Ready renders nothing at card size" is deliberately NOT here — it is the
// frontier card's call-site rule (`FrontierColumns.tsx:239`, `NowScreen.kt`'s
// `stageChip`), and item detail and the Ledger render Ready happily.

/** One stage's treatment: the web `STAGES` table with the light/dark token
 * pair resolved to `Color.kt` constants. Dark `--stage-progress`/
 * `--stage-done` alias `--ember-400`/`--ink-400`, so those two reuse the
 * ramp constants (`urgencyColor`'s precedent). */
private data class StageSpec(
    val label: String,
    val fg: Color,
    val bg: Color,
    val fgDark: Color,
    val bgDark: Color,
    @DrawableRes val glyph: Int? = null,
)

private val STAGES: Map<String, StageSpec> = mapOf(
    "triage" to StageSpec("Triage", Sky600, Sky100, StageTriageFgDark, StageTriageBgDark, R.drawable.ic_inbox),
    "grilling" to StageSpec("Grilling", Amber600, Amber100, StageGrillingFgDark, StageGrillingBgDark),
    "ready" to StageSpec("Ready", Moss600, Moss100, StageReadyFgDark, StageReadyBgDark),
    "in_progress" to StageSpec("In Progress", Ember600, Ember50, Ember400, StageProgressBgDark),
    "blocked" to StageSpec("Blocked", Crimson600, Crimson100, StageBlockedFgDark, StageBlockedBgDark),
    "done" to StageSpec("Done", Ink400, Ink100, Ink400, StageDoneBgDark),
)

/**
 * @param stage the wire word (`"in_progress"`). An unknown word falls back
 *   to triage, exactly as the web's `STAGES[stage] || STAGES.triage` does —
 *   a stage this client has not heard of is by definition unsorted to it.
 * @param dark the same explicit flag `urgencyColor` takes; callers thread
 *   their screen root's `isSystemInDarkTheme()`.
 * @param compact the 8dp dot form for dense rows.
 */
@Composable
fun StageBadge(
    stage: String,
    dark: Boolean,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
) {
    val spec = STAGES[stage] ?: STAGES.getValue("triage")
    val fg = if (dark) spec.fgDark else spec.fg
    val bg = if (dark) spec.bgDark else spec.bg
    val label = spec.label

    if (compact) {
        Box(
            modifier
                .size(8.dp)
                .background(fg, CircleShape)
                .semantics { contentDescription = label },
        )
        return
    }

    if (spec.glyph != null) {
        Row(
            modifier = modifier
                .height(22.dp)
                .background(bg, RoundedCornerShape(percent = 50))
                .padding(horizontal = 6.dp)
                .semantics {
                    role = Role.Image
                    contentDescription = label
                },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                painterResource(spec.glyph),
                contentDescription = null,
                tint = fg,
                modifier = Modifier.size(13.dp),
            )
        }
        return
    }

    Row(
        modifier = modifier
            .height(22.dp)
            .background(bg, RoundedCornerShape(percent = 50))
            .padding(start = 6.dp, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            Modifier
                .size(6.dp)
                .background(fg, CircleShape),
        )
        Text(
            label.uppercase(),
            style = MaterialTheme.typography.labelSmall,
            color = fg,
            maxLines = 1,
        )
    }
}
