package net.twinion.hummingbird

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.theme.Amber500
import net.twinion.hummingbird.ui.theme.Crimson500
import net.twinion.hummingbird.ui.theme.Ember400
import net.twinion.hummingbird.ui.theme.Ember500
import net.twinion.hummingbird.ui.theme.UrgencyOverdueDark
import net.twinion.hummingbird.ui.theme.UrgencySoonDark
import uniffi.hummingbird_ffi_mobile.MobileUrgencyBand
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// M1-6's whole surface (#141/#504): the frontier, decided by
// `hummingbird-ffi-mobile::MobileTaskHost.nowQueue` and rendered verbatim —
// this file never orders, bands or decides an affordance itself (see
// `NowViewModel`'s own doc, and `lib.rs`'s module header for the
// Android-never-calls-per-item-decision-functions asymmetry with web this
// screen is the production instance of). `MainActivity`'s `AppRoot` becomes
// this composable's host (M0's proof screen moves behind the "Status"
// action, M1-6's own scope note); no nav library in M1 — `onShowStatus` is a
// plain callback, the same mode-toggle shape a `NavHost` would later
// replace. `AppRoot` also owns the foreground sync cadence and hands this
// screen its completion via `syncTick` (see that parameter's own note).

/** `YYYY-MM-DDTHH:MM`, the reader's own local wall clock — the shape
 * `hummingbird_core::decisions::urgency::compute_urgency`'s module doc
 * requires from every caller, since that crate resolves no civil date to an
 * instant itself. */
private val DEADLINE_SHAPE: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm")

private fun nowDeadlineShaped(): String = LocalDateTime.now().format(DEADLINE_SHAPE)

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
 * [urgencyColor] uses. */
private fun urgencyLabel(band: MobileUrgencyBand): String = when (band) {
    MobileUrgencyBand.CALM -> "CALM"
    MobileUrgencyBand.SOON -> "SOON"
    MobileUrgencyBand.NOW -> "NOW"
    MobileUrgencyBand.OVERDUE -> "OVERDUE"
}

/** S11/#109's wire vocabulary, mapped to its button label and nothing more
 * — `ItemPanel.tsx`'s `ACTION_BUTTON` verbatim (`client/web/src/components/
 * domain/ItemPanel.tsx:74`; `Mark blocked` says what `Blocked` means, per
 * the design README's voice rule; "Blocked" means an external wait and
 * nothing else). Which actions a row *offers* is decided
 * entirely core-side ([NowItemRecord.availableActions]) — this map only
 * ever renders whatever that list already contains. Shared with
 * `ItemDetailScreen`, which offers the same vocabulary from the same
 * core-decided list — two spellings of "Mark blocked" would be two
 * spellings of one domain word. */
internal val ACTION_LABEL: Map<String, String> = mapOf(
    "start" to "Start",
    "complete" to "Complete",
    "block" to "Mark blocked",
    "cancel" to "Cancel",
)

@Composable
fun NowScreen(onShowStatus: () -> Unit, onShowAlerts: () -> Unit, syncTick: Int = 0) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // Activity-scoped, not composition-scoped: see NowViewModel.factory.
    val viewModel: NowViewModel = viewModel(factory = NowViewModel.factory(context))
    val items by viewModel.items.collectAsState()
    val loading by viewModel.loading.collectAsState()
    val dark = isSystemInDarkTheme()

    suspend fun reload() {
        viewModel.refresh(nowDeadlineShaped())
    }

    LaunchedEffect(Unit) { reload() }

    // Foreground refresh on every return to this screen — independent of
    // `syncTick` below, so a capture or an act taken elsewhere (or on
    // another device) shows up the moment this screen is looked at again,
    // even before the next sync cycle completes.
    LifecycleResumeEffect(Unit) {
        val resumed = scope.launch { reload() }
        onPauseOrDispose { resumed.cancel() }
    }

    // `syncTick` is `AppRoot`'s cadence hand-off (#514 review): the
    // foreground `user`/`timer` sync legs live at the content root now, not
    // on this screen, so this is how Now learns a cycle completed — one
    // whether the tick's own cause was this screen being open or `Status`
    // being open — and re-reads `now_queue` rather than rendering a stale
    // mirror until its own next resume.
    LaunchedEffect(syncTick) {
        if (syncTick > 0) reload()
    }

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                // The product name is lowercase everywhere; the screen
                // title is the one exception the design system already
                // carries (a verb/noun, not the brand).
                Text("Now", style = MaterialTheme.typography.headlineLarge)
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    TextButton(onClick = onShowAlerts) {
                        Text("Alerts")
                    }
                    TextButton(onClick = onShowStatus) {
                        Text("Status")
                    }
                }
            }

            when {
                loading && items.isEmpty() -> CircularProgressIndicator()
                items.isEmpty() -> Text(
                    // Honesty over reassurance (README): an empty frontier
                    // is reported as a fact, not apologised for.
                    "Nothing on the frontier.",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                else -> LazyColumn(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    items(items, key = { it.id }) { record ->
                        NowRow(
                            record = record,
                            dark = dark,
                            onAct = { action ->
                                scope.launch {
                                    viewModel.act(
                                        record.id,
                                        action,
                                        System.currentTimeMillis(),
                                        nowDeadlineShaped(),
                                    )
                                }
                            },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun NowRow(record: NowItemRecord, dark: Boolean, onAct: (String) -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                urgencyColor(record.urgency, dark)?.let { swatch ->
                    Box(
                        modifier = Modifier
                            .size(8.dp)
                            .background(swatch, CircleShape),
                    )
                }
                Text(
                    urgencyLabel(record.urgency),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                record.deadline?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }

            Text(record.title, style = MaterialTheme.typography.bodyLarge)

            if (record.availableActions.isNotEmpty()) {
                // A `FlowRow`, not a `Row`: a `Ready` item offers all four
                // actions (`decisions::actions::available_actions`), and
                // "Start / Complete / Mark blocked / Cancel" is wider than
                // a phone card — a fixed, non-scrolling Row clipped the
                // trailing action on the ordinary case, not just on the
                // Fold's cover display. Wrapping keeps every offered action
                // reachable at any width without this file deciding which
                // ones matter.
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    for (action in record.availableActions) {
                        OutlinedButton(onClick = { onAct(action) }) {
                            Text(ACTION_LABEL[action] ?: action)
                        }
                    }
                }
            }
        }
    }
}
