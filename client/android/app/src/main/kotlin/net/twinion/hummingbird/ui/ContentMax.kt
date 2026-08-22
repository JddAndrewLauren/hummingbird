package net.twinion.hummingbird.ui

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** The web's `--content-max` (`design/tokens/spacing.css`: 880px), hand-
 * ported as an inline dp like every other dimension (ADR-0026 ports colour
 * and type tokens only; there is no gate for spacing). One definition so
 * the bar-tab screens cannot drift apart on it. */
private val CONTENT_MAX = 880.dp

/** Caps a full-width content column at [CONTENT_MAX] and centres the
 * capped column in the leftover — the phone is untouched (the cap is wider
 * than any phone), the unfolded display stops stretching rows across its
 * whole width (`responsive.css`'s `.hb-single-column`, ported). Callers
 * chain it after their fill/inset padding and before their own content
 * padding.
 *
 * [capped] = false is the multi-column screens' opt-out: a screen whose
 * wide form packs lanes or grid columns needs the whole width the cap
 * exists to withhold, so it passes `!LocalWideWindow.current` and keeps
 * the cap — unchanged by construction — on the phone. Everything
 * single-column keeps the default. */
fun Modifier.contentMaxWidth(capped: Boolean = true): Modifier =
    if (capped) {
        wrapContentWidth(Alignment.CenterHorizontally)
            .widthIn(max = CONTENT_MAX)
            .fillMaxWidth()
    } else {
        fillMaxWidth()
    }
