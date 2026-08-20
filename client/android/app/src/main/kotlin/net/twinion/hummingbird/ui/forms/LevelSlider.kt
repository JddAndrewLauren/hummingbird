package net.twinion.hummingbird.ui.forms

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import net.twinion.hummingbird.ui.EnergyGlyph
import net.twinion.hummingbird.ui.LevelGlyphFamily
import net.twinion.hummingbird.ui.SizeGlyph
import net.twinion.hummingbird.ui.levelColor
import uniffi.hummingbird_ffi_mobile.VocabOption

/** #529's first shared form component: a closed-vocabulary field rendered
 * as a row of `FilterChip`s — the capture box's own energy/size Sliders —
 * so the Triage screen (#531) reaches for this rather than growing a third
 * copy. Built on `FilterChip` rather than a hand-rolled `clickable Text`
 * (review finding on #529's own PR): `FilterChip` is what the destination
 * toggle and `PriorityRow` already render this screen's other two rows of
 * choices with, and it supplies `Role.Button` semantics and the platform's
 * 48dp minimum touch target for free — a hand-rolled `Text` chip gave
 * neither.
 *
 * `options` always comes from [uniffi.hummingbird_ffi_mobile.captureFormMeta]
 * — `hummingbird_core::decisions::vocabulary`'s real values, crossed as
 * [VocabOption]s — never a literal Kotlin list. That is the whole point of
 * the door this component was built to consume: a size/energy word is data
 * this screen renders, not a word it knows.
 *
 * Tapping the already-selected option clears the field (`onSelect(null)`) —
 * the same "tap again to clear" gesture `ItemDetailPanel`'s own
 * `VocabularyRow` uses (a *separate*, pre-existing control this component
 * does not replace — see that composable's own header), since deciding a
 * level is mint-time work, never forced.
 *
 * #531 reaches for *this*, not for `NowScreen`'s `FacetChipGroup`/`AxisRow`
 * (#530), which stayed private on purpose: a facet group is multi-select over
 * a `Set<String>` in a wrapping `FlowRow`, this is single-select over a
 * `String?` in a `Row`, and one component spanning both would be abstraction
 * for its own sake. Note the sentinel these two do not agree on: cleared here
 * is `null`, while [PriorityRow] clears to `""` because its own value is a
 * non-null `String` — a caller moving a field between them has to translate.
 *
 * @param glyphFamily when set, each chip draws its level glyph beside its
 *   word (#558, ADR-0024: the value is being *chosen* here, so the word
 *   stays — three unlabelled targets would be a guessing game). Glyph and
 *   word share one ramp colour, always — it is the vocabulary's colour, not
 *   selection state, so it does not defer to the chip's selected tint. The
 *   position is the option's index in the core-supplied list (index + 1):
 *   the list order IS the scale, so no level word appears in this file and
 *   `CaptureFieldSetStructuralTest`'s literal ban holds by construction.
 *   The glyph is decorative (`contentDescription = null`) — the chip's own
 *   label names the choice.
 */
@Composable
fun LevelSlider(
    label: String,
    options: List<VocabOption>,
    selected: String?,
    onSelect: (String?) -> Unit,
    modifier: Modifier = Modifier,
    glyphFamily: LevelGlyphFamily? = null,
) {
    val dark = isSystemInDarkTheme()
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for ((index, option) in options.withIndex()) {
                val isSelected = option.value == selected
                FilterChip(
                    selected = isSelected,
                    onClick = { onSelect(if (isSelected) null else option.value) },
                    label = {
                        if (glyphFamily == null) {
                            Text(option.label)
                        } else {
                            val position = index + 1
                            val colour = levelColor(position, dark)
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(4.dp),
                            ) {
                                when (glyphFamily) {
                                    LevelGlyphFamily.SIZE ->
                                        SizeGlyph(position = position, color = colour, size = 13.dp)
                                    LevelGlyphFamily.ENERGY ->
                                        EnergyGlyph(position = position, color = colour, size = 13.dp)
                                }
                                Text(option.label, color = colour)
                            }
                        }
                    },
                )
            }
        }
    }
}
