package net.twinion.hummingbird.wear.capture

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.wear.compose.material3.FilledIconButton
import androidx.wear.compose.material3.Icon
import androidx.wear.compose.material3.IconButtonDefaults
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.Text
import net.twinion.hummingbird.brand.R
import net.twinion.hummingbird.ui.theme.Ember700
import net.twinion.hummingbird.ui.theme.Ink400
import net.twinion.hummingbird.ui.theme.Sky600

// Where a spoken line lands (the Wear capture design handoff, 2026-09-10):
// the transcript as the chooser returned it — no editing on the watch —
// over the web capture box's three squares as three rounds, in the web's
// order and colours: the inbox on triage's blue, the plus on the accent,
// the calendar-check on ember-700 for "Mint for today". The glyph and the
// colour are the label, so each button's `contentDescription` is the only
// place its gesture is named, with a mono caption beneath for the reader
// who has not learned them yet. `Sky600` and `Ember700` are named directly
// for the phone's reason (`CaptureActivity.kt`'s row): one fill has to
// carry white content, and the scheme's slots are not those blues and
// oranges. Nothing here decides anything about the text: the core's gate
// ran before this screen was drawn, and runs again on submit.
@Composable
internal fun DestinationScreen(
    transcript: String,
    onTriage: () -> Unit,
    onMint: () -> Unit,
    onToday: () -> Unit,
) {
    ScreenScaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                text = transcript,
                style = MaterialTheme.typography.titleLarge,
                color = MaterialTheme.colorScheme.onSurface,
                textAlign = TextAlign.Center,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Destination(R.drawable.ic_inbox, "Triage", "triage", Sky600, onTriage)
                Destination(R.drawable.ic_plus, "Mint action", "add", MaterialTheme.colorScheme.primary, onMint)
                Destination(R.drawable.ic_calendar_check, "Mint for today", "today", Ember700, onToday)
            }
        }
    }
}

@Composable
private fun Destination(
    iconRes: Int,
    name: String,
    caption: String,
    fill: Color,
    onClick: () -> Unit,
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        FilledIconButton(
            onClick = onClick,
            modifier = Modifier.size(IconButtonDefaults.DefaultButtonSize),
            colors = IconButtonDefaults.filledIconButtonColors(
                containerColor = fill,
                contentColor = Color.White,
            ),
        ) {
            Icon(painterResource(iconRes), contentDescription = name, modifier = Modifier.size(24.dp))
        }
        Text(caption, style = MaterialTheme.typography.labelSmall, color = Ink400)
    }
}
