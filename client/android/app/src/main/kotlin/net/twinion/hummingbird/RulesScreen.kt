package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.background
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
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
import uniffi.hummingbird_ffi_mobile.BacktestRecord
import uniffi.hummingbird_ffi_mobile.MobileOperator
import uniffi.hummingbird_ffi_mobile.MobileTier
import uniffi.hummingbird_ffi_mobile.MobileValueWidget
import uniffi.hummingbird_ffi_mobile.RuleConditionInput
import uniffi.hummingbird_ffi_mobile.RuleConditionRecord
import uniffi.hummingbird_ffi_mobile.RuleFieldRecord
import uniffi.hummingbird_ffi_mobile.RuleFormRecord
import uniffi.hummingbird_ffi_mobile.RuleRecord

// The rules surface (#540/M4, ADR-0013): the list, the enable/disable
// toggle, the create-and-edit form, and a draft rule's backtest count.
//
// **This file decides nothing about a rule.** `isValid` gates the invalid
// badge, `legalOperators` fills the operator row, `defaultWidget` picks the
// value control, `belowAlarmInterval` raises the duration warning, and
// `matchCount` is the backtest — every one of them decided in
// `hummingbird_core::decisions::rules` and arriving applied. There is no
// operator table here, no duration grammar, no `23:59`, no comparator and
// no notion of which fields a kind declares. `RulesScreenStructuralTest`
// reads this file to keep it that way, for the same reason
// `AlertsScreenStructuralTest` reads its four: a Kotlin copy of a decision
// compiles, runs, and looks right on every fixture anyone would think to
// write.
//
// **The route has a permanent More-sheet entry since #541.**
//
// The `when`s over the seam's enums carry no `else ->` arm, deliberately:
// an eighth operator or a seventh widget added to ADR-0013's vocabulary is
// then a Kotlin compile error here rather than a control that silently
// renders as nothing.

@Composable
fun RulesScreen(
    syncTick: Int = 0,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: RulesViewModel = viewModel(factory = RulesViewModel.factory(context))
    val state by viewModel.state.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()
    val draft by viewModel.draft.collectAsState()
    val form by viewModel.form.collectAsState()
    val backtest by viewModel.backtest.collectAsState()
    val pendingEnabled by viewModel.pendingEnabled.collectAsState()

    suspend fun reload() = viewModel.load()

    LaunchedEffect(Unit) { reload() }

    LifecycleResumeEffect(Unit) {
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
            TextButton(
                onClick = {
                    if (draft != null) viewModel.discardEdit() else onBack()
                },
            ) {
                Text(if (draft != null) "Cancel" else "Back")
            }

            statusLine?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }

            val editing = draft
            val currentForm = form
            if (editing != null && currentForm != null) {
                EditBody(
                    draft = editing,
                    form = currentForm,
                    backtest = backtest,
                    canSave = viewModel.canSave,
                    onChange = { next -> scope.launch { viewModel.updateDraft(next) } },
                    onBacktest = {
                        scope.launch { viewModel.runBacktest(System.currentTimeMillis()) }
                    },
                    onSave = { scope.launch { viewModel.save(System.currentTimeMillis()) } },
                )
            } else {
                when (val current = state) {
                    RulesState.Loading -> CircularProgressIndicator()
                    RulesState.NotSynced -> RulesNotSyncedBody(
                        onRetry = { scope.launch { reload() } },
                    )
                    is RulesState.Loaded -> ListBody(
                        rules = current.rules,
                        pendingEnabled = pendingEnabled,
                        onNew = { scope.launch { viewModel.beginCreate() } },
                        onEdit = { record -> scope.launch { viewModel.beginEdit(record) } },
                        onSetEnabled = { ruleId, enabled ->
                            scope.launch {
                                viewModel.setEnabled(ruleId, enabled, System.currentTimeMillis())
                            }
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun RulesNotSyncedBody(onRetry: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Not synced yet", style = MaterialTheme.typography.headlineLarge)
        Text(
            "This device hasn't got the rules yet. A sync has already run " +
                "once; try again in a moment.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        OutlinedButton(onClick = onRetry) { Text("Try again") }
    }
}

@Composable
private fun ListBody(
    rules: List<RuleRecord>,
    pendingEnabled: Map<String, Boolean>,
    onNew: () -> Unit,
    onEdit: (RuleRecord) -> Unit,
    onSetEnabled: (String, Boolean) -> Unit,
) {
    Text("Rules", style = MaterialTheme.typography.headlineLarge)

    if (rules.isEmpty()) {
        // Honesty over reassurance: an empty list is a fact, not an
        // apology, and nothing here is broken.
        Text(
            "No rules yet. A rule promotes an event into an alert.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }

    // Rendered in the order the seam handed them over. There is no
    // comparator on this side.
    for (rule in rules) {
        RuleCard(
            rule = rule,
            pending = pendingEnabled[rule.id],
            onEdit = { onEdit(rule) },
            onSetEnabled = { enabled -> onSetEnabled(rule.id, enabled) },
        )
    }

    Button(onClick = onNew) { Text("New rule") }
}

@Composable
private fun RuleCard(
    rule: RuleRecord,
    /** A switch position tapped but not yet handed back, or null when the
     * row and the switch agree — [RulesViewModel.pendingEnabled]. */
    pending: Boolean?,
    onEdit: () -> Unit,
    onSetEnabled: (Boolean) -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(rule.name, style = MaterialTheme.typography.titleMedium)
                // The toggle is one CAS field. `enabled` is read for the
                // toggle and for nothing else — a disabled rule is not an
                // invalid one, and the two facts arrive separately. The
                // tapped position wins over the row's until the write lands,
                // and the badge below says which one this is.
                Switch(checked = pending ?: rule.enabled, onCheckedChange = onSetEnabled)
            }

            if (pending != null) {
                Text(
                    "Not synced yet — this device is still holding that change.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }

            val meta = buildList {
                add(kindLabel(rule.kindLabelKey).uppercase())
                add("SEVERITY:${rule.severity.uppercase()}")
                add("TIER:${tierLabel(rule.tier).uppercase()}")
            }
            Text(
                meta.joinToString(" · "),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            if (rule.severityIsUnranked) {
                // A severity outside `SEVERITIES` ranks at 0, so ADR-0014's
                // ratchet can never lift it. The seam decided that; this
                // only says so.
                Text(
                    "This severity isn't one the ratchet ranks.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }

            if (!rule.isValid) {
                Badge("INVALID")
                Text(
                    "Names ${rule.invalidFields.joinToString(", ")} — this rule " +
                        "can't fire until that's fixed.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }

            for (condition in rule.conditions) {
                ConditionLine(condition)
            }

            OutlinedButton(onClick = onEdit) { Text("Edit") }
        }
    }
}

@Composable
private fun ConditionLine(condition: RuleConditionRecord) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        val negation = if (condition.negate) "not " else ""
        Text(
            "${condition.field} $negation${operatorLabel(condition.op)} " +
                condition.valueDisplay,
            style = MaterialTheme.typography.bodyMedium,
        )
        if (condition.belowAlarmInterval) {
            // Warn, never reject (#138): a rule that fires less precisely
            // than its author intended is still legitimate.
            Text(
                "Shorter than the sweep interval — this may fire late.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun EditBody(
    draft: RuleDraft,
    form: RuleFormRecord,
    backtest: BacktestRecord?,
    canSave: Boolean,
    onChange: (RuleDraft) -> Unit,
    onBacktest: () -> Unit,
    onSave: () -> Unit,
) {
    Text(
        if (draft.ruleId == null) "New rule" else "Edit rule",
        style = MaterialTheme.typography.headlineLarge,
    )

    OutlinedTextField(
        value = draft.name,
        onValueChange = { onChange(draft.copy(name = it)) },
        label = { Text("Name") },
        // Whether a name counts as one is the core's answer, never this
        // file's.
        isError = !canSave,
        modifier = Modifier.fillMaxWidth(),
    )

    // The kinds the registry declares, in its own order, with ADR-0013's
    // "any kind" first — the seam assembled that list; this only draws it.
    ChoiceRow(
        label = "KIND",
        options = form.kindOptions.map { it.key to kindLabel(it.labelKey) },
        selected = draft.eventKind,
        // Changing the kind empties the conditions, as the web form does: a
        // row naming the old kind's field renders operator-less here and the
        // seam refuses the save it leads to ("… is not a field this kind
        // declares"), with nothing on screen saying which row to remove.
        onSelect = { key -> onChange(draft.copy(eventKind = key, conditions = emptyList())) },
    )

    ChoiceRow(
        label = "SEVERITY",
        options = form.severities.map { it to it },
        selected = draft.severity,
        onSelect = { value -> value?.let { onChange(draft.copy(severity = it)) } },
    )

    ChoiceRow(
        label = "TIER",
        options = form.tiers.map { tierLabel(it) to tierLabel(it) },
        selected = tierLabel(draft.tier),
        onSelect = { value ->
            form.tiers.firstOrNull { tierLabel(it) == value }
                ?.let { onChange(draft.copy(tier = it)) }
        },
    )

    Text(
        "CONDITIONS",
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )

    draft.conditions.forEachIndexed { index, condition ->
        ConditionEditor(
            condition = condition,
            form = form,
            onChange = { next ->
                onChange(
                    draft.copy(
                        conditions = draft.conditions.toMutableList()
                            .also { it[index] = next },
                    ),
                )
            },
            onRemove = {
                onChange(
                    draft.copy(
                        conditions = draft.conditions.toMutableList()
                            .also { it.removeAt(index) },
                    ),
                )
            },
        )
    }

    // A fresh row opens on the first field the kind offers, at the operator
    // the seam says that field's type defaults to — never a hand-picked
    // pair that could be illegal the moment it appears.
    form.fields.firstOrNull()?.let { first ->
        OutlinedButton(
            onClick = {
                onChange(
                    draft.copy(
                        conditions = draft.conditions + RuleConditionInput(
                            field = first.name,
                            op = first.legalOperators.first(),
                            value = "",
                            negate = false,
                        ),
                    ),
                )
            },
        ) {
            Text("Add condition")
        }
    }

    BacktestPanel(backtest = backtest, onRun = onBacktest)

    Button(onClick = onSave, enabled = canSave) { Text("Save") }
}

@Composable
private fun ConditionEditor(
    condition: RuleConditionInput,
    form: RuleFormRecord,
    onChange: (RuleConditionInput) -> Unit,
    onRemove: () -> Unit,
) {
    val field = form.fields.firstOrNull { it.name == condition.field }
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            ChoiceRow(
                label = "FIELD",
                options = form.fields.map { it.name to it.name },
                selected = condition.field,
                onSelect = { name ->
                    val next = form.fields.firstOrNull { it.name == name } ?: return@ChoiceRow
                    // Re-picking a field re-legalises the row: the new
                    // type's own first legal operator, and an empty value.
                    // Which operators those are is the seam's answer.
                    onChange(
                        condition.copy(
                            field = next.name,
                            op = next.legalOperators.first(),
                            value = "",
                        ),
                    )
                },
            )

            field?.let {
                ChoiceRow(
                    label = "OPERATOR",
                    options = it.legalOperators.map { op -> operatorLabel(op) to operatorLabel(op) },
                    selected = operatorLabel(condition.op),
                    onSelect = { label ->
                        it.legalOperators.firstOrNull { op -> operatorLabel(op) == label }
                            ?.let { op -> onChange(condition.copy(op = op)) }
                    },
                )
            }

            OutlinedTextField(
                value = condition.value,
                onValueChange = { onChange(condition.copy(value = it)) },
                label = { Text(valueLabel(field)) },
                supportingText = field?.let { { Text(valueHint(it)) } },
                modifier = Modifier.fillMaxWidth(),
            )

            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text("NOT", style = MaterialTheme.typography.labelSmall)
                Switch(
                    checked = condition.negate,
                    onCheckedChange = { onChange(condition.copy(negate = it)) },
                )
                TextButton(onClick = onRemove) { Text("Remove") }
            }
        }
    }
}

@Composable
private fun BacktestPanel(backtest: BacktestRecord?, onRun: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        OutlinedButton(onClick = onRun) { Text("Backtest") }
        backtest?.let {
            if (it.isAvailable) {
                Text(
                    "${it.matchCount} would have matched",
                    style = MaterialTheme.typography.bodyMedium,
                )
                // Never a bare count: the corpus caveat travels with it.
                Text(
                    corpusNote(it.corpusNoteKey),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                Text(
                    "No local history for this kind — nothing to backtest against.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** One choice field as a row of words — `ItemDetailPanel`'s own
 * `VocabularyRow`, widened to carry a key beside its label so the kind
 * picker can offer ADR-0013's null kind as a real option. Deliberately a
 * private copy rather than a shared component: #529 is where the phone's
 * form components get a home, and building a speculative one here would
 * pre-empt it. */
@Composable
private fun ChoiceRow(
    label: String,
    options: List<Pair<String?, String>>,
    selected: String?,
    onSelect: (String?) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        // A `FlowRow`, not a `Row`, for `NowScreen`'s reason: KIND draws every
        // kind the registry declares plus ADR-0013's null one, and FIELD draws
        // every field a kind offers — both past the width of a phone, and a
        // fixed `Row` clips what does not fit rather than wrapping it, leaving
        // the trailing choices invisible and untappable.
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            for ((key, wording) in options) {
                if (key == selected) {
                    Button(onClick = { onSelect(key) }) { Text(wording) }
                } else {
                    OutlinedButton(onClick = { onSelect(key) }) { Text(wording) }
                }
            }
        }
    }
}

@Composable
private fun Badge(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onErrorContainer,
        modifier = Modifier
            .background(MaterialTheme.colorScheme.errorContainer, RoundedCornerShape(6.dp))
            .padding(horizontal = 8.dp, vertical = 2.dp),
    )
}

// -- rendering, and only rendering ------------------------------------------
//
// ADR-0025's verdict table puts these on this side of the line explicitly:
// the web keeps `OPERATOR_LABELS` and `kindLabel` as TS, for the same
// reason. Two clients wording "is within the next" differently is a
// difference, not a bug — unlike two clients disagreeing about which
// operators are legal, which is what the seam decides.

/** Human copy for a kind key. Falls back to the raw key for one this build
 * has no wording for yet — a kind can be added to the registry with no
 * UI-side change and must still render as something legible. */
private fun kindLabel(key: String): String = when (key) {
    "any_kind" -> "Any kind"
    "email" -> "Email"
    "calendar_event" -> "Calendar event"
    "item_threshold" -> "Item"
    "snapshot_change" -> "Snapshot change"
    "alert_raised" -> "Alert raised"
    else -> key
}

private fun operatorLabel(op: MobileOperator): String = when (op) {
    MobileOperator.EQ -> "is"
    MobileOperator.CONTAINS -> "contains"
    MobileOperator.GT -> "is greater than"
    MobileOperator.LT -> "is less than"
    MobileOperator.IS -> "is"
    MobileOperator.WITHIN_NEXT -> "is within the next"
    MobileOperator.WITHIN_LAST -> "was within the last"
}

private fun tierLabel(tier: MobileTier): String = when (tier) {
    MobileTier.URGENT -> "urgent"
    MobileTier.NORMAL -> "normal"
}

/** What the value control is called, given the widget the seam picked for
 * this field. The widget is decided by `rules::widget_for`; only the
 * wording is here. */
private fun valueLabel(field: RuleFieldRecord?): String = when (field?.defaultWidget) {
    MobileValueWidget.CHIPS -> "Values, comma separated"
    MobileValueWidget.DURATION -> "Duration"
    MobileValueWidget.DATETIME -> "Duration"
    MobileValueWidget.BOOLEAN -> "true or false"
    MobileValueWidget.NUMBER -> "Number"
    MobileValueWidget.TEXT -> "Value"
    null -> "Value"
}

/** The hint beneath the value field. The duration units come from the
 * record — a `date` field is day-grained only (ADR-0013), and that
 * narrowing is the seam's answer, not a Kotlin rule. */
private fun valueHint(field: RuleFieldRecord): String = when (field.defaultWidget) {
    MobileValueWidget.DURATION,
    MobileValueWidget.DATETIME,
    -> "A number and one of ${field.durationUnits.joinToString(", ")}"
    MobileValueWidget.CHIPS -> "Any one of them matches"
    MobileValueWidget.BOOLEAN -> "true or false"
    MobileValueWidget.NUMBER -> "A number"
    MobileValueWidget.TEXT -> "Matching ignores case"
}

private fun corpusNote(key: String): String = when (key) {
    "backtest_corpus_frontier_only" ->
        "Counted against this device's frontier only — items still in triage, " +
            "or blocked, aren't in it."
    else -> key
}
