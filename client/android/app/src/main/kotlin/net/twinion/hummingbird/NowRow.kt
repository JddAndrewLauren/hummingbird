package net.twinion.hummingbird

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter
import net.twinion.hummingbird.ui.EnergyGlyph
import net.twinion.hummingbird.ui.SizeGlyph
import net.twinion.hummingbird.ui.StageBadge
import net.twinion.hummingbird.ui.energyTitle
import net.twinion.hummingbird.ui.levelColor
import net.twinion.hummingbird.ui.levelPosition
import net.twinion.hummingbird.ui.sizeTitle
import net.twinion.hummingbird.ui.theme.Amber500
import net.twinion.hummingbird.ui.theme.Crimson500
import net.twinion.hummingbird.ui.theme.Ember400
import net.twinion.hummingbird.ui.theme.Ember500
import net.twinion.hummingbird.ui.theme.Moss600
import net.twinion.hummingbird.ui.theme.StatusDoneFgDark
import net.twinion.hummingbird.ui.theme.UrgencyOverdueDark
import net.twinion.hummingbird.ui.theme.UrgencySoonDark
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.NowItemRecord
import uniffi.hummingbird_ffi_mobile.TriageItemRecord

// The one compact item card (extracted from NowScreen.kt for the Triage
// parity slice): title first, then the conditional meta "pills" row —
// urgency dot + word, deadline, stage chip, word-free size/energy glyphs —
// and the trailing mark-done check. Two surfaces render it (Now's frontier
// columns and the Triage queue), which is exactly why it lives in its own
// file: the calm-gets-nothing, ready-says-nothing, judged-only-glyph rules
// must not fork per surface.
//
// [NowRowModel] is the card's own contract: both seam records already carry
// every field DECIDED (urgency banding included, `TriageItemRecord.urgency`
// since the Triage-parity slice), so the adapters below copy fields
// verbatim and decide nothing — the "applied results only" rule
// (`ffi-mobile/src/lib.rs`'s module doc) holds on this side of the seam
// too. SIZE_VALUES/ENERGY_VALUES stay in NowScreen.kt (the facet chips'
// copy, pinned against the core by ffi-mobile's
// `the_now_screen_facet_vocabularies_match_the_core`); this file reads the
// same lists rather than declaring rivals.

/** `YYYY-MM-DDTHH:MM`, the reader's own local wall clock — the shape
 * `hummingbird_core::decisions::urgency::compute_urgency`'s module doc
 * requires from every caller, since that crate resolves no civil date to an
 * instant itself. */
private val DEADLINE_SHAPE: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm")

internal fun nowDeadlineShaped(): String = LocalDateTime.now().format(DEADLINE_SHAPE)

/** [MobileUrgencyBand]'s dot colour, or `null` for the band that gets no
 * dot — exhaustive, no `else` arm: the compile-time drift gate the brief
 * names for a `uniffi::Enum` crossing. Values are the design mirror's
 * `--urgency-*` tokens (`.claude/skills/hummingbird-design/tokens/colors.css`),
 * light/dark split exactly `HummingbirdTheme`'s own mapping notes state the
 * pattern for.
 *
 * `CALM` maps to `null` rather than to a grey, because ADR-0021 decision 2
 * is explicit: "`calm` gets no swatch — the default is not a claim worth
 * colouring". Encoding that here rather than at the call site keeps the
 * rule with the mapping the rule is about, and keeps the `when` the one
 * place a new band has to be answered for. */
private fun urgencyColor(band: MobileUrgencyBand, dark: Boolean): Color? = when (band) {
    MobileUrgencyBand.CALM -> null
    MobileUrgencyBand.SOON -> if (dark) UrgencySoonDark else Amber500
    MobileUrgencyBand.NOW -> if (dark) Ember400 else Ember500
    MobileUrgencyBand.OVERDUE -> if (dark) UrgencyOverdueDark else Crimson500
}

/** [MobileUrgencyBand]'s mono-meta label (README: "UPPERCASE only in the
 * 11px mono meta style") — exhaustive, no `else` arm, the same discipline
 * [urgencyColor] uses.
 *
 * `CALM` maps to `null` for the same reason it takes no swatch: ADR-0021
 * decision 2 says "the default is not a claim worth colouring", and a card
 * that spends its most prominent meta slot printing "CALM" claims exactly
 * that. The word is the non-colour carrier for the three bands that *do*
 * make a claim, so only this arm goes. */
private fun urgencyLabel(band: MobileUrgencyBand): String? = when (band) {
    MobileUrgencyBand.CALM -> null
    MobileUrgencyBand.SOON -> "SOON"
    MobileUrgencyBand.NOW -> "NOW"
    MobileUrgencyBand.OVERDUE -> "OVERDUE"
}

/** Exactly what the compact card draws, no more — see the file header for
 * why both adapters must stay verbatim field copies. */
internal data class NowRowModel(
    val title: String,
    val urgency: MobileUrgencyBand,
    val deadline: String?,
    val stage: String,
    val size: String?,
    val energy: String?,
    val canMarkDone: Boolean,
)

internal fun NowItemRecord.asRowModel(): NowRowModel = NowRowModel(
    title = title,
    urgency = urgency,
    deadline = deadline,
    stage = stage,
    size = size,
    energy = energy,
    canMarkDone = canMarkDone,
)

internal fun TriageItemRecord.asRowModel(): NowRowModel = NowRowModel(
    title = title,
    urgency = urgency,
    deadline = deadline,
    stage = stage,
    size = size,
    energy = energy,
    canMarkDone = canMarkDone,
)

@Composable
internal fun NowRow(
    record: NowRowModel,
    dark: Boolean,
    selected: Boolean,
    onOpen: () -> Unit,
    onComplete: () -> Unit,
) {
    // The card is the door to the item's expanded panel, in place above
    // the board (#521's tap target, retargeted from the full-screen route
    // by the inline-expansion slice). Material3's `onClick` overload
    // rather than a `clickable` modifier: it carries the ripple, the
    // `role = Button` semantics and the minimum touch target that a bare
    // modifier leaves to the caller. No chevron and no other added chrome
    // -- the design system's `interactive` card is the whole affordance,
    // and its icon vocabulary has no chevron in it.
    //
    // No action ROW on the card: acting is what the opened item is for,
    // and the four-button FlowRow was most of the card's ~130dp height.
    // The one inline affordance is the web `ItemRow`'s own mark-done
    // checkmark (`MarkDoneButton.tsx`), trailing and gated on the seam's
    // `canMarkDone` — an earlier slice left it out as a mis-tap risk
    // beside the whole-card target, reversed on operator feedback
    // (2026-08-19): `IconButton`'s 48dp minimum target is its own slip
    // margin. `availableActions` still arrives decided on the record;
    // the opened item renders it.
    Card(
        onClick = onOpen,
        modifier = Modifier
            .fillMaxWidth()
            .semantics { this.selected = selected },
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        // The originating card stays marked while its panel stands — an
        // ember-tinted BORDER, never a fill (the design README's accent
        // rule; the web ItemCard's `accent` + `aria-current` treatment).
        border = if (selected) {
            BorderStroke(1.dp, MaterialTheme.colorScheme.primary)
        } else {
            null
        },
    ) {
        Row(
            // --space-5 / --space-2: the web ItemCard's own density class,
            // not the 16/8 a full-width content card gets.
            modifier = Modifier.padding(12.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                // Title first, meta below — the web phone `ItemRow`'s own wrap
                // order (`.hb-item-row-title` takes the first line; the mono
                // chips wrap under it).
                Text(record.title, style = MaterialTheme.typography.bodyLarge)
                // Every entry on the meta row is conditional -- the swatch and the
                // word both go for `calm`, the deadline draws only when set, and
                // `ready` says nothing -- so the ordinary minted action reaches
                // this row with nothing to put on it. An empty `Row` is not free:
                // it measures zero high but the Column's `spacedBy(4.dp)` still
                // pays for it, stranding 4dp below the title. Read once here and
                // reused below, so the guard cannot disagree with what draws.
                val swatch = urgencyColor(record.urgency, dark)
                val urgencyWord = urgencyLabel(record.urgency)
                // `ItemRow`'s own `item.stage === "ready" ? null : <StageBadge>`
                // (web), ported: `Ready` alone says nothing at card size.
                val stageChip = if (record.stage == "ready") null else record.stage
                // Word-free glyph positions (#558, ADR-0024): only a judged,
                // known dimension draws on a card — an absent one is omitted
                // entirely (that omission is what licenses dropping the word),
                // and an unknown wire word maps to position 0, which a card
                // never draws (the unset ghost is detail-only).
                val sizePos = record.size?.let { levelPosition(SIZE_VALUES, it) }?.takeIf { it > 0 }
                val energyPos = record.energy?.let { levelPosition(ENERGY_VALUES, it) }?.takeIf { it > 0 }
                if (swatch != null || urgencyWord != null || record.deadline != null ||
                    stageChip != null || sizePos != null || energyPos != null
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        swatch?.let {
                            // 6dp: the web ItemRow's own dot size.
                            Box(
                                modifier = Modifier
                                    .size(6.dp)
                                    .background(it, CircleShape),
                            )
                        }
                        urgencyWord?.let { label ->
                            Text(
                                label,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        record.deadline?.let {
                            Text(
                                it,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        // The stage chip IS the triage label: a capture riding
                        // inline into these columns is marked by the app's one
                        // stage vocabulary, never a badge invented for this
                        // surface (`ItemRow`'s own rule, ported) — and since
                        // #557 the one treatment too, `ui/StageBadge.kt`.
                        stageChip?.let {
                            StageBadge(stage = it, dark = dark)
                        }
                        // ADR-0024: drawn, not written — the glyph names itself
                        // (`Size: quick`) since no word sits beside it here.
                        sizePos?.let { pos ->
                            SizeGlyph(
                                position = pos,
                                color = levelColor(pos, dark),
                                contentDescription = sizeTitle(record.size),
                            )
                        }
                        energyPos?.let { pos ->
                            EnergyGlyph(
                                position = pos,
                                color = levelColor(pos, dark),
                                contentDescription = energyTitle(record.energy),
                            )
                        }
                    }
                }
            }

            if (record.canMarkDone) {
                IconButton(onClick = onComplete) {
                    Icon(
                        painterResource(R.drawable.ic_check),
                        contentDescription = "Mark \"${record.title}\" done",
                        modifier = Modifier.size(18.dp),
                        // The mark-done green — the web MarkDoneButton's
                        // own token, `--status-done-fg` (the status green),
                        // and its documented exception to "icons never
                        // carry colour independently of their label".
                        tint = if (dark) StatusDoneFgDark else Moss600,
                    )
                }
            }
        }
    }
}
