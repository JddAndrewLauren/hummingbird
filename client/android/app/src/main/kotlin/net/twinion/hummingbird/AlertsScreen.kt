package net.twinion.hummingbird

import android.app.NotificationManager
import android.content.Intent
import android.provider.Settings
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationManagerCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.contentMaxWidth
import net.twinion.hummingbird.ui.theme.Crimson100
import net.twinion.hummingbird.ui.theme.Crimson600
import net.twinion.hummingbird.ui.theme.CrimsonDark
import net.twinion.hummingbird.ui.theme.LocalHbDark
import net.twinion.hummingbird.ui.theme.Sky100
import net.twinion.hummingbird.ui.theme.Sky600
import net.twinion.hummingbird.ui.theme.StatusDangerBgDark
import net.twinion.hummingbird.ui.theme.StatusInfoBgDark
import net.twinion.hummingbird.ui.theme.StatusInfoFgDark
import uniffi.hummingbird_ffi_mobile.AlertRecord

// The alerts surface (M2/#141, ADR-0012): every live alert, in the order
// the core put them in, each offering the Ack gesture the core says is
// worth offering.
//
// This file decides nothing about an alert. `AlertsViewModel`'s own doc
// carries the reasoning; the two consequences visible here are that
// `canAck` — not a `dismissedAt` test, which cannot tell an
// expired-then-re-raised occurrence from an acked one — gates the Ack
// button, and that the list is rendered in arrival order with no
// comparator, because `Core::live_alerts` already sorted it (`raised_at`
// descending, id as tiebreak). A structural test refuses both re-derivations.
//
// The health rows are the don't-fail-silently half. Two device conditions
// stop alerts arriving as intended and neither raises an error anywhere:
// notifications switched off for the app, and no notification-policy
// access, which makes the urgent channel's DND bypass inert. Both are
// reported as passive facts that tap through to the relevant Settings
// screen — honesty over reassurance, and never a dialog fight.

/** The chip's colour pair, by the alert's own `severity`.
 *
 * **Severity, not Tier.** Tier weighs a *delivery* — the Rule assigns it
 * when the alert rings — and is deliberately not a property of the record
 * (CONTEXT.md's own distinction), so it is not on an [AlertRecord] and
 * cannot be shown on a row read from the mirror. Severity is what the
 * record carries.
 *
 * This is a presentation map over a wire vocabulary and nothing more, the
 * same shape as `ItemDetailPanel`'s `ACTION_LABEL`: it ranks nothing. Severity
 * *ranking* is a core decision with three server-side surfaces
 * (`domain::severity`), none of which has a colour to disagree with. An
 * unknown or absent severity takes the quiet tone rather than the loud one
 * — the same never-guess-upward rule the channel fallback uses. */
internal fun severityTone(severity: String?, dark: Boolean): Pair<Color, Color> =
    when (severity) {
        "urgent", "high" ->
            if (dark) CrimsonDark to StatusDangerBgDark else Crimson600 to Crimson100
        else ->
            if (dark) StatusInfoFgDark to StatusInfoBgDark else Sky600 to Sky100
    }

/** The mono-meta chip label. UPPERCASE is the 11px mono meta style and the
 * only place the design system allows it; an alert with no severity says
 * so rather than borrowing one. */
internal fun severityLabel(severity: String?): String = severity?.uppercase() ?: "NO SEVERITY"

/** Acked rows stay legible but recede — `AlertCard.tsx`'s own `0.55`,
 * hand-ported like every other value the web card carries (ADR-0026). */
private const val ACKED_ALPHA = 0.55f

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AlertsScreen(
    syncTick: Int = 0,
    isRefreshing: Boolean = false,
    onRefresh: () -> Unit = {},
    onOpenAlert: (String) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // Activity-scoped, not composition-scoped: see AlertsViewModel.factory.
    val viewModel: AlertsViewModel = viewModel(factory = AlertsViewModel.factory(context))
    val alerts by viewModel.alerts.collectAsState()
    val loading by viewModel.loading.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()
    val dark = LocalHbDark.current

    suspend fun reload() {
        viewModel.refresh(System.currentTimeMillis())
    }

    LaunchedEffect(Unit) { reload() }

    // Refresh on every return to this screen, independent of the sync
    // cadence — an ack taken from the notification shade, or on another
    // device, shows up the moment this screen is looked at again.
    LifecycleResumeEffect(Unit) {
        val resumed = scope.launch { reload() }
        onPauseOrDispose { resumed.cancel() }
    }

    // `AppRoot`'s cadence hand-off (#514's shape): one increment per
    // completed sync cycle, so this screen re-reads the mirror after each
    // one rather than showing a stale list until its own next resume.
    LaunchedEffect(syncTick) {
        if (syncTick > 0) reload()
    }

    Scaffold { padding ->
        // The pull gesture is a second door onto AppRoot's one sync cadence
        // (`sync("user")` via [onRefresh]) — never a screen-local cycle; the
        // reload itself still arrives through `syncTick` when the cycle lands.
        PullToRefreshBox(
            isRefreshing = isRefreshing,
            onRefresh = onRefresh,
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .contentMaxWidth()
                    // Top 12dp, not the outer 24dp: with the title gone the
                    // health rows sit directly under the app row.
                    .padding(start = 24.dp, top = 12.dp, end = 24.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                NotificationHealthRows()

                statusLine?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.error,
                    )
                }

                when {
                    // The no-list branches still sit in a scrollable so the pull
                    // gesture works from an empty or still-loading lane too.
                    loading && alerts.isEmpty() -> Column(
                        modifier = Modifier
                            .fillMaxSize()
                            .verticalScroll(rememberScrollState()),
                    ) {
                        CircularProgressIndicator()
                    }
                    alerts.isEmpty() -> Column(
                        modifier = Modifier
                            .fillMaxSize()
                            .verticalScroll(rememberScrollState()),
                    ) {
                        Text(
                            // An empty lane is good news reported as a fact, and
                            // the reason it is empty is worth saying: default-deny
                            // means silence is the designed resting state.
                            "Nothing is ringing. What no rule matches stays silent.",
                            style = MaterialTheme.typography.bodyLarge,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    else -> LazyColumn(
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                        // The last row scrolls clear of the Capture FAB.
                        contentPadding = PaddingValues(bottom = 64.dp),
                    ) {
                        items(alerts, key = { it.id }) { record ->
                            AlertRow(
                                record = record,
                                dark = dark,
                                onOpen = { onOpenAlert(record.id) },
                                onAck = {
                                    scope.launch {
                                        viewModel.ack(record.id, System.currentTimeMillis())
                                    }
                                },
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun AlertRow(
    record: AlertRecord,
    dark: Boolean,
    onOpen: () -> Unit,
    onAck: () -> Unit,
) {
    // `canAck` is the core's decided answer to "is this gesture worth
    // offering" — a live, not-yet-dismissed alert. Acking a settled row is
    // legal on the authority but is not something to show.
    val actionable = record.canAck
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .alpha(if (actionable) 1f else ACKED_ALPHA)
            .clickable(onClick = onOpen),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface,
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
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
                    // The middle dot is the metadata join, not an icon.
                    "${record.source} · ${record.sourceKey}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }

            Text(record.title, style = MaterialTheme.typography.bodyLarge)
            record.body?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                )
            }

            if (actionable) {
                OutlinedButton(onClick = onAck) {
                    // "Ack", never "Dismiss" — they mean different things.
                    Text("Ack")
                }
            } else {
                Text(
                    "Acked",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** Up to two passive rows, each stating one device condition that would
 * stop alerts arriving as intended, and tapping through to the Settings
 * screen that fixes it. Neither condition raises an error anywhere else —
 * a notification posted with notifications disabled simply vanishes — so
 * this is the only place either becomes visible. */
@Composable
private fun NotificationHealthRows() {
    val context = LocalContext.current
    val notificationsOn = NotificationManagerCompat.from(context).areNotificationsEnabled()
    val policyAccess = context.getSystemService(NotificationManager::class.java)
        ?.isNotificationPolicyAccessGranted == true

    if (!notificationsOn) {
        HealthRow(
            text = "Notifications are off for hummingbird. Alerts won't ring.",
            onClick = {
                context.startActivity(
                    Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                        .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                )
            },
        )
    }
    if (!policyAccess) {
        HealthRow(
            text = "Urgent alerts won't cut through Do Not Disturb without policy access.",
            onClick = {
                context.startActivity(
                    Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                )
            },
        )
    }
}

@Composable
private fun HealthRow(text: String, onClick: () -> Unit) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick),
        colors = CardDefaults.cardColors(
            // `--surface-quiet`: where a surface needs to recede.
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Text(
            text,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(12.dp),
        )
    }
}
