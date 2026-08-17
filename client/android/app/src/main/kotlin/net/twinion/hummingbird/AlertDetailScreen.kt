package net.twinion.hummingbird

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import uniffi.hummingbird_ffi_mobile.AlertRecord

// One alert, in full (M2/#141) — where a notification tap lands, and where
// the Ack gesture has room to be deliberate rather than a shade action.
//
// Like `AlertsScreen`, this file decides nothing: `canAck` gates the
// button, `isLive` is read rather than re-derived, and `dismissedAt` is
// never tested here (ADR-0014's predicate is the reason; a structural test
// enforces it). The one state worth its own branch is `NotSynced` — see
// `AlertDetailViewModel`'s doc for why absence is a temporary condition and
// not an error.

@Composable
fun AlertDetailScreen(
    alertId: String,
    syncTick: Int = 0,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: AlertDetailViewModel =
        viewModel(factory = AlertDetailViewModel.factory(context))
    val state by viewModel.state.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()
    val dark = isSystemInDarkTheme()

    suspend fun reload() {
        viewModel.load(alertId, System.currentTimeMillis())
    }

    LaunchedEffect(alertId) { reload() }

    LifecycleResumeEffect(alertId) {
        val resumed = scope.launch { reload() }
        onPauseOrDispose { resumed.cancel() }
    }

    LaunchedEffect(syncTick) {
        if (syncTick > 0) reload()
    }

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            TextButton(onClick = onBack) {
                Text("Back")
            }

            statusLine?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }

            when (val current = state) {
                AlertDetailState.Loading -> CircularProgressIndicator()
                AlertDetailState.NotSynced -> NotSyncedBody(
                    onRetry = { scope.launch { reload() } },
                )
                is AlertDetailState.Loaded -> LoadedBody(
                    record = current.record,
                    dark = dark,
                    onOpenSource = { url ->
                        context.startActivity(
                            Intent(Intent.ACTION_VIEW, Uri.parse(url))
                                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                        )
                    },
                    onAck = {
                        scope.launch {
                            viewModel.ack(alertId, System.currentTimeMillis())
                        }
                    },
                )
            }
        }
    }
}

@Composable
private fun NotSyncedBody(onRetry: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Not synced yet", style = MaterialTheme.typography.headlineLarge)
        Text(
            // Honesty over reassurance: say what is true and what to do,
            // and do not pretend the alert is gone or broken.
            "This device hasn't got this alert yet. A sync has already run " +
                "once; try again in a moment.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        OutlinedButton(onClick = onRetry) {
            Text("Try again")
        }
    }
}

@Composable
private fun LoadedBody(
    record: AlertRecord,
    dark: Boolean,
    onOpenSource: (String) -> Unit,
    onAck: () -> Unit,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        val (fg, bg) = severityTone(record.severity, dark)
        Text(
            severityLabel(record.severity),
            style = MaterialTheme.typography.labelSmall,
            color = fg,
            modifier = Modifier
                .background(bg, RoundedCornerShape(6.dp))
                .padding(horizontal = 8.dp, vertical = 2.dp),
        )
        Text(
            "${record.source} · ${record.sourceKey}",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }

    Text(record.title, style = MaterialTheme.typography.headlineLarge)

    record.body?.let {
        Text(it, style = MaterialTheme.typography.bodyLarge)
    }

    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            // The mono meta style: data the system computed. `isLive` is
            // the core's decided verdict, printed rather than recomputed.
            Text(
                "V${record.version} · ${if (record.isLive) "LIVE" else "SETTLED"}",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            record.subjectKey?.let {
                Text(
                    "SUBJECT $it",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }

    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (record.canAck) {
            Button(onClick = onAck) {
                Text("Ack")
            }
        } else {
            Text(
                "Acked",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        record.url?.let { url ->
            OutlinedButton(onClick = { onOpenSource(url) }) {
                Text("Open source")
            }
        }
    }
}
