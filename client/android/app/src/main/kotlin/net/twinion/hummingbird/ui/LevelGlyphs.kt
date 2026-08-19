package net.twinion.hummingbird.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import net.twinion.hummingbird.ui.theme.Amber500
import net.twinion.hummingbird.ui.theme.Ember400
import net.twinion.hummingbird.ui.theme.Ember500
import net.twinion.hummingbird.ui.theme.Ink400
import net.twinion.hummingbird.ui.theme.Moss600
import net.twinion.hummingbird.ui.theme.StatusDoneFgDark
import net.twinion.hummingbird.ui.theme.UrgencySoonDark

// ADR-0024 decision 2's Compose port (#558): size and energy are **drawn,
// not written** — size as depth rings (a centre dot gaining rings as the
// work goes deeper), energy as three ascending bars. The level is carried
// twice over: by the glyph's fill (earned elements solid, unearned ghosted,
// unset a flat wash) and by colour from ONE four-step ramp indexed by
// **position on the scale, never by value name** — which is what lets one
// table serve both dimensions instead of two that drift. Geometry and the
// three opacity stops are verbatim from the web's
// `client/web/src/components/core/custom-glyphs.tsx` and the ramp from
// `client/web/src/screens/size-energy.ts`; copied, not re-derived.
//
// Which Android surfaces draw the glyph, decided at #558 (the decision
// ADR-0024 left to this slice; it matches the ADR's own web ruling, so a
// header note is the record):
//  - The Now card draws the glyph **with no word**, and omits an absent
//    dimension entirely — the omission is what licenses dropping the word,
//    because `size-unset` and `size-deep` are the same three rings told
//    apart by opacity alone, so a word-free glyph is only ever a *judged*
//    glyph (web's frontier card, `FrontierColumns.tsx`).
//  - Item detail's read mode draws glyph **beside** its word, and is the
//    ONLY surface that renders the unset ghost (beside an em dash).
//  - The capture and triage `LevelSlider` chips draw the glyph beside its
//    word: the value is being chosen there, not reported, and three
//    unlabelled targets would be a guessing game. Glyph and word share one
//    colour (icons never carry colour independently of their label).
//  - The board's facet **filter** chips keep their words unchanged — they
//    are filters over the vocabulary, not item annotations.
//
// The stroke width is a private constant, deliberately not a parameter: the
// web accepts-and-ignores a caller's `strokeWidth` only for Icon-signature
// parity, and honouring one would thicken the ghosted rings along with the
// earned ones. Do not "restore" it.

private const val UNEARNED = 0.25f
private const val EARNED = 1f
private const val UNSET = 0.45f

/** All geometry lives in the web glyphs' 24-unit viewBox and scales with
 * the composable's `size`. */
private const val VIEWBOX = 24f
private const val RING_STROKE = 2.5f

/** ONE ramp per scheme, indexed by position 0..3 (0 = unset). The four
 * colours reuse tokens the system already has — `--text-muted`,
 * `--status-done-fg`, `--urgency-soon`, `--urgency-now` — all already in
 * `Color.kt` under `ColorTokenDriftTest` (ADR-0026); this file mints no
 * colour of its own. */
private val RAMP_LIGHT = listOf(Ink400, Moss600, Amber500, Ember500)
private val RAMP_DARK = listOf(Ink400, StatusDoneFgDark, UrgencySoonDark, Ember400)

/** The two dimensions a level glyph can draw. A `LevelSlider` caller names
 * the family; the ramp itself never needs to know which one asked. */
enum class LevelGlyphFamily { SIZE, ENERGY }

/** The ramp colour for a position on the scale — `levelColor` in
 * `size-energy.ts`, ported. Position, never a value name: both
 * vocabularies resolve through this one table. */
fun levelColor(position: Int, dark: Boolean): Color =
    (if (dark) RAMP_DARK else RAMP_LIGHT)[position.coerceIn(0, 3)]

/** Position on the scale from the vocabulary's own core-pinned order:
 * `indexOf + 1`, with `null` and an unknown word both landing on 0
 * (unset). The list IS the position source — `SIZE_VALUES`, `ENERGY_VALUES`
 * and `VocabOption` order are all pinned against the core
 * (`the_now_screen_facet_vocabularies_match_the_core`), so no value-name
 * table exists here to drift from them. */
fun levelPosition(vocabulary: List<String>, value: String?): Int {
    if (value == null) return 0
    val index = vocabulary.indexOf(value)
    return if (index < 0) 0 else index + 1
}

/** The word-free glyph's accessible name — `sizeTitle` in `size-energy.ts`.
 * Sentence case, not the label's uppercase: this is read aloud or shown as
 * a description, and neither wants shouting. */
fun sizeTitle(value: String?): String = "Size: ${value ?: "not judged"}"

fun energyTitle(value: String?): String = "Energy: ${value ?: "not judged"}"

private fun stop(element: Int, position: Int): Float = when {
    position == 0 -> UNSET
    element <= position -> EARNED
    else -> UNEARNED
}

private fun Modifier.glyphSemantics(name: String?): Modifier =
    if (name == null) {
        this // Decorative: the word is right beside it.
    } else {
        semantics {
            role = Role.Image
            contentDescription = name
        }
    }

/** Size as depth rings: a filled centre dot (element 1), an inner ring (2)
 * and an outer ring (3). `custom-glyphs.tsx`'s `SizeRings`, verbatim:
 * radii 3 / 6.75 / 10.5, stroke 2.5, all in viewBox units. */
@Composable
fun SizeGlyph(
    position: Int,
    color: Color,
    modifier: Modifier = Modifier,
    size: Dp = 16.dp,
    contentDescription: String? = null,
) {
    Canvas(
        modifier
            .size(size)
            .glyphSemantics(contentDescription),
    ) {
        val s = this.size.minDimension / VIEWBOX
        val centre = Offset(12f * s, 12f * s)
        drawCircle(color.copy(alpha = color.alpha * stop(1, position)), radius = 3f * s, center = centre)
        drawCircle(
            color.copy(alpha = color.alpha * stop(2, position)),
            radius = 6.75f * s,
            center = centre,
            style = Stroke(RING_STROKE * s),
        )
        drawCircle(
            color.copy(alpha = color.alpha * stop(3, position)),
            radius = 10.5f * s,
            center = centre,
            style = Stroke(RING_STROKE * s),
        )
    }
}

/** Energy as three ascending bars. `custom-glyphs.tsx`'s `EnergyBars`,
 * verbatim: x 4 / 9.75 / 15.5, y 14 / 9 / 4, width 4.5, heights 6 / 11 /
 * 16, corner radius 2, in viewBox units. */
@Composable
fun EnergyGlyph(
    position: Int,
    color: Color,
    modifier: Modifier = Modifier,
    size: Dp = 16.dp,
    contentDescription: String? = null,
) {
    Canvas(
        modifier
            .size(size)
            .glyphSemantics(contentDescription),
    ) {
        val s = this.size.minDimension / VIEWBOX
        drawBar(Rect(Offset(4f * s, 14f * s), Size(4.5f * s, 6f * s)), color, stop(1, position), s)
        drawBar(Rect(Offset(9.75f * s, 9f * s), Size(4.5f * s, 11f * s)), color, stop(2, position), s)
        drawBar(Rect(Offset(15.5f * s, 4f * s), Size(4.5f * s, 16f * s)), color, stop(3, position), s)
    }
}

private fun DrawScope.drawBar(rect: Rect, color: Color, alphaStop: Float, scale: Float) {
    drawRoundRect(
        color = color.copy(alpha = color.alpha * alphaStop),
        topLeft = rect.topLeft,
        size = rect.size,
        cornerRadius = CornerRadius(2f * scale),
    )
}
