package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.ChoiceRow
import uniffi.hummingbird_ffi_mobile.ItemStepRecord
import uniffi.hummingbird_ffi_mobile.MobileGrillCompletion
import uniffi.hummingbird_ffi_mobile.MobileGrillTurn
import uniffi.hummingbird_ffi_mobile.MobileGrillTurnState
import uniffi.hummingbird_ffi_mobile.MobileGrillVerdict
import uniffi.hummingbird_ffi_mobile.MobileProposedEditRow
import uniffi.hummingbird_ffi_mobile.formatGrillTranscript
import uniffi.hummingbird_ffi_mobile.grillDemotesFromFrontier
import uniffi.hummingbird_ffi_mobile.grillFrontierDemotionWarning
import uniffi.hummingbird_ffi_mobile.grillPlanReplacementLabel
import uniffi.hummingbird_ffi_mobile.grillProposalRows
import uniffi.hummingbird_ffi_mobile.grillWouldStrandPlan

// The Grill takeover (#355/#539, ADR-0023): the one-typed-question-at-a-time
// interview as a takeover rather than a panel, mounted over two rows —
// item detail's own Grill button and the Triage row's, whose button has
// been rendering gated off since M3 (`TriageScreen.kt`'s own note).
//
// This screen renders the current `MobileGrillTurnState` and nothing else;
// every decision — the turn lane, the draft, the Steps snapshot, the
// Confirm mutation — belongs to [GrillTakeoverViewModel]. The review card's
// own predicates (`wouldStrandPlan`/`demotesFromFrontier`/
// `planReplacementLabel`) are the sunk `decisions::skills::review` family,
// reached through the `grill*` uniffi doors — no re-derivation here.
@Composable
fun GrillTakeoverScreen(
    itemId: String,
    backLabel: String,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: GrillTakeoverViewModel = viewModel(factory = GrillTakeoverViewModel.factory(context))
    val state by viewModel.state.collectAsState()
    var confirmingDiscard by rememberSaveable { mutableStateOf(false) }

    LaunchedEffect(itemId) {
        viewModel.open(itemId, System.currentTimeMillis())
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
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                TextButton(onClick = onBack) { Text(backLabel) }
                if (!confirmingDiscard) {
                    OutlinedButton(onClick = { confirmingDiscard = true }) { Text("Discard") }
                }
            }

            if (confirmingDiscard) {
                DiscardConfirmationRow(
                    onKeep = { confirmingDiscard = false },
                    onDiscard = {
                        confirmingDiscard = false
                        // Pop only once the delete has landed (#565
                        // review): the pop clears this entry's
                        // `ViewModelStore`, and a still-suspended discard
                        // would be cancelled with it — see
                        // `GrillTakeoverViewModel.discard`.
                        scope.launch {
                            viewModel.discard(itemId, System.currentTimeMillis())
                            onBack()
                        }
                    },
                )
            }

            when (val current = state) {
                GrillTakeoverState.Loading -> CircularProgressIndicator()
                is GrillTakeoverState.Ready -> {
                    Text("grilling — ${current.item.title}", style = MaterialTheme.typography.labelSmall)
                    when (val turn = current.turn) {
                        MobileGrillTurnState.Idle -> {}
                        is MobileGrillTurnState.Asking -> Narration(turn.messages)
                        is MobileGrillTurnState.Question -> QuestionCard(
                            prompt = turn.question.prompt,
                            recommendedAnswer = turn.question.recommendedAnswer,
                            choices = turn.question.choices,
                            onAnswer = { text -> viewModel.answer(itemId, text) },
                        )
                        is MobileGrillTurnState.Proposal -> ReviewCard(
                            stage = current.item.stage,
                            steps = current.sessionSteps,
                            summary = turn.proposal.summary,
                            verdict = turn.proposal.verdict,
                            // Decided core-side (#595, ADR-0025): the rows
                            // are the patch as words; the JSON itself still
                            // travels whole below, untouched. remember():
                            // the seam crossing (a JSON parse + a record
                            // clone) prices per proposal, not per
                            // recomposition.
                            proposedEdit = remember(turn.proposal, current.item) {
                                grillProposalRows(turn.proposal.patchJson, current.item)
                            },
                            turns = current.turns,
                            confirming = current.confirming,
                            completionError = current.completionError,
                            onKeepGrilling = { viewModel.keepGrilling(itemId) },
                            onConfirm = { summary, deleteUntickedPlan ->
                                scope.launch {
                                    val ok = viewModel.confirm(
                                        itemId,
                                        MobileGrillCompletion(
                                            transcript = formatGrillTranscript(current.turns),
                                            summary = summary,
                                            verdict = turn.proposal.verdict,
                                            modelProposal = turn.proposal.patchJson,
                                            // Identical to modelProposal by
                                            // construction (#595): Android
                                            // ships no inline edit, so what
                                            // was proposed is what is
                                            // recorded — web's editable
                                            // textarea is the affordance
                                            // this client does not have.
                                            appliedPatch = turn.proposal.patchJson,
                                            deleteUntickedPlan = deleteUntickedPlan,
                                        ),
                                        System.currentTimeMillis(),
                                    )
                                    if (ok) onBack()
                                }
                            },
                        )
                        is MobileGrillTurnState.Declined -> DeclinedCard(
                            messages = turn.messages,
                            reason = turn.reason,
                            onRetry = { viewModel.retry(itemId) },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun DiscardConfirmationRow(onKeep: () -> Unit, onDiscard: () -> Unit) {
    // #576: the prompt sits above its two answers rather than beside them,
    // and the answers wrap. In a plain `Row` the sentence took the width
    // and `Keep` — the escape from a destructive question — collapsed to an
    // unreadable sliver, leaving system Back as the only way out.
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            "Discard this grill? The interview so far will be lost.",
            style = MaterialTheme.typography.bodyMedium,
        )
        ChoiceRow {
            Button(onClick = onDiscard) { Text("Discard") }
            OutlinedButton(onClick = onKeep) { Text("Keep") }
        }
    }
}

@Composable
private fun Narration(messages: List<String>) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        for (message in messages) {
            Text(
                message,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun QuestionCard(
    prompt: String,
    recommendedAnswer: String,
    choices: List<String>,
    onAnswer: (String) -> Unit,
) {
    // Saveable, not `remember`: a fold/rotation mid-answer must not throw
    // away what was typed — the same rule `ItemDetailScreen.kt`'s own edit
    // draft follows.
    var freeText by rememberSaveable(prompt) { mutableStateOf("") }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(prompt, style = MaterialTheme.typography.headlineSmall)
        Text(
            "Recommended: $recommendedAnswer",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        // #576: `grill-me` returns full-sentence choices, so two of them
        // never share a phone's width — and a plain `Row` did not clip the
        // second one, it stood it up as a letter column three screens tall
        // and pushed the free-text field and `Answer` below the fold.
        ChoiceRow {
            for (choice in choices) {
                OutlinedButton(onClick = { onAnswer(choice) }) { Text(choice) }
            }
        }
        OutlinedTextField(
            value = freeText,
            onValueChange = { freeText = it },
            label = { Text("Or answer in your own words") },
            modifier = Modifier.fillMaxWidth(),
        )
        Button(
            onClick = { if (freeText.isNotBlank()) onAnswer(freeText) },
            enabled = freeText.isNotBlank(),
        ) {
            Text("Answer")
        }
    }
}

@Composable
private fun ReviewCard(
    stage: String,
    steps: List<ItemStepRecord>,
    summary: String,
    verdict: MobileGrillVerdict,
    proposedEdit: List<MobileProposedEditRow>,
    turns: List<MobileGrillTurn>,
    confirming: Boolean,
    completionError: String?,
    onKeepGrilling: () -> Unit,
    onConfirm: (summary: String, deleteUntickedPlan: Boolean) -> Unit,
) {
    // Saveable: the review card's own edits are human-authored content, the
    // same standard `ItemDetailScreen.kt`'s edit draft and `GrillTakeover
    // .tsx`'s web precedent both hold themselves to.
    var summaryDraft by rememberSaveable(summary) { mutableStateOf(summary) }
    var deleteUntickedPlan by rememberSaveable(summary, proposedEdit) { mutableStateOf(false) }

    val offerPlanReplacement = grillWouldStrandPlan(verdict, steps)

    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(
            if (verdict == MobileGrillVerdict.RESOLVED) "Resolved" else "Fog remains",
            style = MaterialTheme.typography.labelSmall,
        )
        if (grillDemotesFromFrontier(verdict, stage)) {
            Text(
                grillFrontierDemotionWarning(),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
            )
        }
        OutlinedTextField(
            value = summaryDraft,
            onValueChange = { summaryDraft = it },
            label = { Text("Summary") },
            modifier = Modifier.fillMaxWidth(),
        )
        // #595: the proposed edit renders as labelled rows decided
        // core-side (`grill_proposal_rows`), never as escaped JSON on the
        // one screen that asks for a decision. Read-only: Android ships no
        // inline edit anywhere, so Confirm records the proposal unchanged —
        // the web keeps its editable textarea because its edit affordance
        // is real (`GrillTakeover.tsx`).
        Text("Proposed edit", style = MaterialTheme.typography.labelSmall)
        if (proposedEdit.isEmpty()) {
            // `fog_remains` commonly carries an empty patch — a stated
            // fact, not a blank (design README: empty states are facts).
            Text(
                "No item edits proposed.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            for (row in proposedEdit) {
                // The strikethrough carries which value is which only for
                // eyes; the merged description says it in words, so TalkBack
                // reads "Size: proposed deep, was normal" rather than three
                // bare values on the screen that asks for a decision.
                val spoken = buildString {
                    append(row.label)
                    append(": proposed ")
                    append(row.proposed)
                    row.current?.let { append(", was ").append(it) }
                }
                Column(
                    modifier = Modifier.semantics(mergeDescendants = true) {
                        contentDescription = spoken
                    },
                ) {
                    Text(
                        row.label,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    row.current?.let { currentValue ->
                        Text(
                            currentValue,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            textDecoration = TextDecoration.LineThrough,
                        )
                    }
                    Text(row.proposed, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
        Text(
            // The web hint, verbatim (`GrillTakeover.tsx`): Confirm records
            // the edit on the Grill; nothing writes it onto the item.
            "Recorded on the Grill — never applied to the item automatically.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (offerPlanReplacement) {
            Row {
                Checkbox(checked = deleteUntickedPlan, onCheckedChange = { deleteUntickedPlan = it })
                Text(grillPlanReplacementLabel(steps), style = MaterialTheme.typography.bodySmall)
            }
        }
        if (completionError != null) {
            Text(completionError, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onKeepGrilling, enabled = !confirming) { Text("Keep grilling") }
            Button(
                onClick = { onConfirm(summaryDraft, deleteUntickedPlan) },
                enabled = !confirming,
            ) {
                Text("Confirm")
            }
        }
    }
}

@Composable
private fun DeclinedCard(messages: List<String>, reason: String, onRetry: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Narration(messages)
        // Verbatim, unprefixed, unbranched — #307 made the seam's decline
        // prose-only, precisely so nothing string-matches it.
        Text(reason, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error)
        Row(horizontalArrangement = Arrangement.End) {
            OutlinedButton(onClick = onRetry) { Text("Try again") }
        }
    }
}
