package net.twinion.hummingbird.wear.questions

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.Text
import uniffi.hummingbird_ffi_mobile.MobileTaskHost

// The standing-questions route (ADR-0039). This slice is the shell only —
// the ranked list over `rank_panes(Now)` lands with the questions step; until
// then the route says what it is rather than drawing an empty list, which
// would read as "nothing to answer" and be a lie.

@Composable
internal fun QuestionsScreen(core: MobileTaskHost?, syncTick: Int) {
    ScreenScaffold { padding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = if (core == null) "Loading" else "Questions",
                style = MaterialTheme.typography.titleMedium,
            )
        }
    }
}
