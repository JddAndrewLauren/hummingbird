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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import uniffi.hummingbird_ffi_mobile.ItemStepRecord
import uniffi.hummingbird_ffi_mobile.MobileGrillCompletion
import uniffi.hummingbird_ffi_mobile.MobileGrillTurn
import uniffi.hummingbird_ffi_mobile.MobileGrillTurnState
import uniffi.hummingbird_ffi_mobile.MobileGrillVerdict
import uniffi.hummingbird_ffi_mobile.formatGrillTranscript
import uniffi.hummingbird_ffi_mobile.grillDemotesFromFrontier
import uniffi.hummingbird_ffi_mobile.grillFrontierDemotionWarning
import uniffi.hummingbird_ffi_mobile.grillPlanReplacementLabel
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
                        viewModel.discard(itemId, System.currentTimeMillis())
                        onBack()
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
                            patchJson = turn.proposal.patchJson,
                            turns = current.turns,
                            confirming = current.confirming,
                            completionError = current.completionError,
                            onKeepGrilling = { viewModel.keepGrilling(itemId) },
                            onConfirm = { summary, patchJson, deleteUntickedPlan ->
                                scope.launch {
                                    val ok = viewModel.confirm(
                                        itemId,
                                        MobileGrillCompletion(
                                            transcript = formatGrillTranscript(current.turns),
                                            summary = summary,
                                            verdict = turn.proposal.verdict,
                                            modelProposal = turn.proposal.patchJson,
                                            appliedPatch = patchJson,
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
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            "Discard this grill? The interview so far will be lost.",
            style = MaterialTheme.typography.bodyMedium,
        )
        Button(onClick = onDiscard) { Text("Discard") }
        OutlinedButton(onClick = onKeep) { Text("Keep") }
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
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
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
    patchJson: String,
    turns: List<MobileGrillTurn>,
    confirming: Boolean,
    completionError: String?,
    onKeepGrilling: () -> Unit,
    onConfirm: (summary: String, patchJson: String, deleteUntickedPlan: Boolean) -> Unit,
) {
    // Saveable: the review card's own edits are human-authored content, the
    // same standard `ItemDetailScreen.kt`'s edit draft and `GrillTakeover
    // .tsx`'s web precedent both hold themselves to.
    var summaryDraft by rememberSaveable(summary) { mutableStateOf(summary) }
    var patchDraft by rememberSaveable(patchJson) { mutableStateOf(patchJson) }
    var deleteUntickedPlan by rememberSaveable(summary, patchJson) { mutableStateOf(false) }

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
        OutlinedTextField(
            value = patchDraft,
            onValueChange = { patchDraft = it },
            label = { Text("Proposed edit") },
            modifier = Modifier.fillMaxWidth(),
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
                onClick = { onConfirm(summaryDraft, patchDraft, deleteUntickedPlan) },
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
