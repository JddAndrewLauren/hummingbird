package net.twinion.hummingbird.ui.forms

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import uniffi.hummingbird_ffi_mobile.VocabOption

/** #529's first shared form component: a closed-vocabulary field rendered
 * as a row of choices, the capture box's own energy/size Sliders and the
 * item-edit form's `VocabularyRow` reshaped into one reusable composable —
 * so the Triage screen (#531) reaches for this rather than growing a third
 * copy.
 *
 * `options` always comes from [uniffi.hummingbird_ffi_mobile.captureFormMeta]
 * — `hummingbird_core::decisions::vocabulary`'s real values, crossed as
 * [VocabOption]s — never a literal Kotlin list. That is the whole point of
 * the door this component was built to consume: a size/energy word is data
 * this screen renders, not a word it knows.
 *
 * Tapping the already-selected option clears the field (`onSelect(null)`) —
 * the same "tap again to clear" gesture `ItemDetailScreen`'s
 * `VocabularyRow` already uses, since deciding a level is mint-time work,
 * never forced.
 */
@Composable
fun LevelSlider(
    label: String,
    options: List<VocabOption>,
    selected: String?,
    onSelect: (String?) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (option in options) {
                val isSelected = option.value == selected
                Text(
                    option.label,
                    style = MaterialTheme.typography.labelLarge,
                    color = if (isSelected) {
                        MaterialTheme.colorScheme.onPrimary
                    } else {
                        MaterialTheme.colorScheme.onSurface
                    },
                    modifier = Modifier
                        .clickable {
                            onSelect(if (isSelected) null else option.value)
                        }
                        .background(
                            color = if (isSelected) {
                                MaterialTheme.colorScheme.primary
                            } else {
                                MaterialTheme.colorScheme.surfaceVariant
                            },
                            shape = RoundedCornerShape(8.dp),
                        )
                        .padding(horizontal = 12.dp, vertical = 6.dp),
                )
            }
        }
    }
}
