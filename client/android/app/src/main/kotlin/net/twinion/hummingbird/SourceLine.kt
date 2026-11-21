package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import net.twinion.hummingbird.notify.SourceIcons
import uniffi.hummingbird_ffi_mobile.AlertRecord

/** What an alert's list card and detail screen say about where it came from
 * (#922, ADR-0042): the source's display name and glyph, then the context
 * line when the source sent one. Replaces the raw `source · source_key`
 * string, which named neither the app nor the thing it was about. The name,
 * icon and context are all decided on the other side of the seam
 * (`to_alert_record`); this only lays them out. */
internal fun sourceLine(record: AlertRecord): String =
    // The middle dot is the metadata join, not an icon.
    record.context?.let { "${record.sourceName} · $it" } ?: record.sourceName

@Composable
internal fun SourceLine(record: AlertRecord, singleLine: Boolean, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Icon(
            painterResource(SourceIcons.drawableFor(record.sourceIcon)),
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            sourceLine(record),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = if (singleLine) 1 else Int.MAX_VALUE,
            overflow = if (singleLine) TextOverflow.Ellipsis else TextOverflow.Clip,
        )
    }
}
