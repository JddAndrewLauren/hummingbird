package net.twinion.hummingbird.wear

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.wear.compose.material3.Button
import androidx.wear.compose.material3.FilledTonalButton
import androidx.wear.compose.material3.Icon
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.Text
import net.twinion.hummingbird.brand.R

// The watch's home (ADR-0039): one large ember button that captures, two
// smaller tonal buttons that open the items by urgency and the standing
// questions (the tile's two glyph rounds, as words — the 2026-09-10 design
// handoff), and beneath them at most one line — either the token is missing or refused ("Send a token
// from the phone"), or the mirror is old enough to say so. The line is the
// design README's honesty rule on a 1.2-inch display: when data is stale
// the product says so and keeps showing it; when nothing needs saying, the
// screen is the two buttons and nothing else.
//
// The column scrolls: on the Pixel Watch 4 (213dp across, smaller than the
// Wear AVD) the three buttons fill the face and the line sat clipped below
// it, invisible, on the first hardware pass (2026-09-10). Centred when it
// fits, reachable by a swipe when it does not — `verticalScroll` alone gives
// the column an unbounded height and so no centre, hence the measured
// `heightIn(min = viewport)` that restores it.
//
// Ember is the one accent and the capture button is its one use here; the
// other two are tonal on purpose. `feather` is the brand's own verb
// for capture (the design README's icon vocabulary).

@Composable
internal fun HomeScreen(
    needsToken: Boolean,
    syncAgeLine: String?,
    onCapture: () -> Unit,
    onItems: () -> Unit,
    onQuestions: () -> Unit,
) {
    ScreenScaffold { padding ->
        BoxWithConstraints(modifier = Modifier.fillMaxSize().padding(padding)) {
            val viewport = maxHeight
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .heightIn(min = viewport)
                    .padding(horizontal = 20.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterVertically),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
            Button(
                onClick = onCapture,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 64.dp),
                icon = { Icon(painterResource(R.drawable.ic_feather), contentDescription = null) },
                label = { Text("Capture") },
            )
            FilledTonalButton(
                onClick = onItems,
                modifier = Modifier.fillMaxWidth(),
                icon = { Icon(painterResource(R.drawable.ic_zap), contentDescription = null) },
                label = { Text("Items") },
            )
            FilledTonalButton(
                onClick = onQuestions,
                modifier = Modifier.fillMaxWidth(),
                icon = { Icon(painterResource(R.drawable.ic_help_circle), contentDescription = null) },
                label = { Text("Questions") },
            )
            val line = if (needsToken) "Send a token from the phone" else syncAgeLine
            if (line != null) {
                Text(
                    text = line,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.padding(top = 4.dp),
                )
            }
            }
        }
    }
}

private const val HOUR_MS = 60L * 60L * 1000L

/** The home line's age words, or `null` when the mirror is fresh enough to
 * say nothing. Never synced is a fact worth a line ("not synced yet"); a
 * sync inside the hour is not — the hourly leg is the promise, and the
 * screen only speaks when it is broken. Hours are floored: "synced 1h ago"
 * covers sixty to a hundred and nineteen minutes, the same rounding the
 * design README's `12m ago` register uses. Pure, and pinned by
 * `HomeScreenWordsTest`. */
internal fun syncAgeLine(lastInformativeAtMs: Long?, nowMs: Long): String? {
    if (lastInformativeAtMs == null) return "not synced yet"
    val age = nowMs - lastInformativeAtMs
    if (age < HOUR_MS) return null
    val hours = age / HOUR_MS
    return "synced ${hours}h ago"
}
