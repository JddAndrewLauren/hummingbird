package net.twinion.hummingbird.wear.items

import android.util.Log
import androidx.compose.foundation.BorderStroke
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.wear.compose.foundation.lazy.ScalingLazyColumn
import androidx.wear.compose.foundation.lazy.items
import androidx.wear.compose.foundation.lazy.rememberScalingLazyListState
import androidx.wear.compose.material3.Card
import androidx.wear.compose.material3.FilledTonalButton
import androidx.wear.compose.material3.Icon
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.Text
import androidx.wear.remote.interactions.RemoteActivityHelper
import kotlinx.coroutines.launch
import net.twinion.hummingbird.brand.R
import net.twinion.hummingbird.core.ItemLink
import net.twinion.hummingbird.ui.theme.AccentQuietBorderDark
import net.twinion.hummingbird.ui.theme.Ink400
import uniffi.hummingbird_ffi_mobile.NowItemRecord

// Items by urgency (the Wear capture design handoff, 2026-09-10): one card
// per frontier item in the core's order — overdue, then due, then soon,
// then calm — each a dot in the urgency colour, the mono line saying when
// (`urgencyRowLabel`), the title, and the context and size when set. Tap
// expands in place, one card at a time: the description (fetched then, from
// `itemDetail`), the accent hairline the design system gives "the answer on
// screen", and one button — "Open on phone", an item link carried to the
// paired phone by `RemoteActivityHelper`. The watch reads; it does not
// edit, mark done or ack (ADR-0039). A second tap folds the card. Reloaded
// on every `syncTick`, so the rows re-read the mirror after each completed
// cycle and never on their own clock — the day the labels say "today"
// against is the one the board was read on (`ItemsViewModel.Loaded.today`).

@Composable
internal fun ItemsScreen(syncTick: Int) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel = remember { ItemsViewModel.create(context) }
    val loaded by viewModel.loaded.collectAsStateWithLifecycle()
    val open by viewModel.open.collectAsStateWithLifecycle()
    // A core that throws must not take the list down: the row stays as it
    // was and the failure is a logcat line, as the tile service treats the
    // same read.
    LaunchedEffect(syncTick) {
        try {
            viewModel.load(System.currentTimeMillis())
        } catch (failed: Exception) {
            Log.w(TAG, "items could not read the mirror", failed)
        }
    }
    val remote = remember { RemoteActivityHelper(context) }

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
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            item { ListHeaderLine("ITEMS · BY URGENCY") }
            if (current.rows.isEmpty()) {
                item {
                    Text(
                        "Nothing on the frontier.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        textAlign = TextAlign.Center,
                    )
                }
            }
            items(current.rows, key = { it.id }) { row ->
                val openRow = open?.takeIf { it.itemId == row.id }
                ItemRow(
                    row = row,
                    today = current.today,
                    open = openRow,
                    onToggle = {
                        scope.launch {
                            try {
                                viewModel.toggle(row.id, current.nowMs)
                            } catch (failed: Exception) {
                                Log.w(TAG, "item detail could not be read", failed)
                            }
                        }
                    },
                    onOpenOnPhone = { openOnPhone(remote, context, row.id) },
                )
            }
        }
    }
}

/** The list's mono heading — the `hb-meta` register centred over the
 * cards, as the design draws it. */
@Composable
internal fun ListHeaderLine(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
        modifier = Modifier.fillMaxWidth().padding(bottom = 2.dp),
    )
}

@Composable
private fun ItemRow(
    row: NowItemRecord,
    today: String,
    open: ItemsViewModel.Open?,
    onToggle: () -> Unit,
    onOpenOnPhone: () -> Unit,
) {
    Card(
        onClick = onToggle,
        modifier = Modifier.fillMaxWidth(),
        // The accent hairline is the open card's alone.
        border = if (open != null) BorderStroke(1.dp, AccentQuietBorderDark) else null,
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(8.dp).background(urgencyDot(row.urgency), CircleShape))
                Text(
                    urgencyRowLabel(row.urgency, row.deadline, today),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
            }
            Text(
                row.title,
                style = MaterialTheme.typography.titleSmall,
                maxLines = if (open != null) Int.MAX_VALUE else 3,
                overflow = TextOverflow.Ellipsis,
            )
            val meta = itemMetaLine(row.context, row.size)
            if (meta != null) {
                Text(meta, style = MaterialTheme.typography.labelSmall, color = Ink400, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            if (open != null) {
                val description = open.description
                if (description != null && description.isNotEmpty()) {
                    Text(
                        description,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(top = 4.dp),
                    )
                }
                FilledTonalButton(
                    onClick = onOpenOnPhone,
                    modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
                    icon = { Icon(painterResource(R.drawable.ic_smartphone), contentDescription = null, modifier = Modifier.size(16.dp)) },
                    label = { Text("Open on phone", style = MaterialTheme.typography.labelMedium) },
                )
            }
        }
    }
}

/** Hands the item to the paired phone: `ItemLink`'s VIEW intent, carried by
 * the Wear remote-interactions helper (one per screen — each instance owns
 * an executor thread) to whichever node is paired. Fire and
 * log — the helper's future resolves on the phone's side of the Data Layer
 * and there is nothing for the watch to draw about it; a failure (no phone
 * in reach, the app not installed there) is a line in logcat, not a dialog
 * the wrist has room for. */
private fun openOnPhone(remote: RemoteActivityHelper, context: android.content.Context, itemId: String) {
    val future = remote.startRemoteActivity(ItemLink.intent(itemId))
    future.addListener(
        {
            try {
                future.get()
            } catch (failed: Exception) {
                Log.w(TAG, "open on phone failed for item $itemId", failed)
            }
        },
        context.mainExecutor,
    )
}

private const val TAG = "hummingbird.wear"
