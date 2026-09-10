package net.twinion.hummingbird.wear.questions

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.wear.compose.foundation.lazy.ScalingLazyColumn
import androidx.wear.compose.foundation.lazy.items
import androidx.wear.compose.foundation.lazy.rememberScalingLazyListState
import androidx.wear.compose.material3.Card
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.Text
import net.twinion.hummingbird.ui.panes.bandColor
import net.twinion.hummingbird.ui.panes.paneHeadline
import uniffi.hummingbird_ffi_mobile.MobileRankedPane

// The standing-questions list (ADR-0039): one row per Now question, in the
// core's salience order, each a band dot, the roster's label and the pane's
// one-line headline (`paneHeadline`, `:brand`'s words — the phone's row
// says the same). Tap expands the facts in place (`WearPaneExpanded`); a
// second tap folds them. Reloaded on every `syncTick`, so the rows re-read
// the mirror after each completed cycle and never on their own clock — the
// clock the words age against is the one the rank was taken at
// (`QuestionsViewModel.Loaded.nowMs`).
//
// No Settings door on the watch: an unbound question shows its sentence
// and stops, because the binding is written on a keyboard.

@Composable
internal fun QuestionsScreen(syncTick: Int) {
    val context = LocalContext.current
    val viewModel = remember { QuestionsViewModel.create(context) }
    val loaded by viewModel.loaded.collectAsStateWithLifecycle()
    val expanded by viewModel.expanded.collectAsStateWithLifecycle()
    LaunchedEffect(syncTick) { viewModel.load(System.currentTimeMillis()) }

    val listState = rememberScalingLazyListState()
    ScreenScaffold(scrollState = listState) { padding ->
        val current = loaded
        if (current == null) {
            Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.Center) {
                Text("Loading", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            return@ScreenScaffold
        }
        ScalingLazyColumn(
            modifier = Modifier.fillMaxSize(),
            state = listState,
            contentPadding = padding,
        ) {
            if (current.panes.isEmpty()) {
                item {
                    Text(
                        "Every question is switched off.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            items(current.panes, key = { it.paneKey }) { pane ->
                QuestionRow(
                    pane = pane,
                    label = viewModel.label(pane),
                    nowMs = current.nowMs,
                    expanded = pane.paneKey in expanded,
                    onToggle = { viewModel.toggle(pane.paneKey) },
                )
            }
        }
    }
}

@Composable
private fun QuestionRow(
    pane: MobileRankedPane,
    label: String,
    nowMs: Long,
    expanded: Boolean,
    onToggle: () -> Unit,
) {
    Card(onClick = onToggle, modifier = Modifier.fillMaxWidth()) {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                // The watch is always dark, so the dark arm is the only one.
                Box(
                    Modifier
                        .size(8.dp)
                        .background(bandColor(pane.answer.band, dark = true), CircleShape),
                )
                Text(
                    label,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Text(
                paneHeadline(pane, nowMs),
                style = MaterialTheme.typography.titleSmall,
                maxLines = if (expanded) Int.MAX_VALUE else 2,
                overflow = TextOverflow.Ellipsis,
            )
            if (expanded) {
                WearPaneExpanded(pane, nowMs)
            }
        }
    }
}
