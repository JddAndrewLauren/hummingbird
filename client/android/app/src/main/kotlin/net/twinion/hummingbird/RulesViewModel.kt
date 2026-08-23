package net.twinion.hummingbird

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlin.random.Random
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.core.WallClock
import net.twinion.hummingbird.sync.SyncWorker
import uniffi.hummingbird_ffi_mobile.BacktestRecord
import uniffi.hummingbird_ffi_mobile.FieldPatch
import uniffi.hummingbird_ffi_mobile.MobileTier
import uniffi.hummingbird_ffi_mobile.RuleConditionInput
import uniffi.hummingbird_ffi_mobile.RuleFormRecord
import uniffi.hummingbird_ffi_mobile.RuleRecord
import uniffi.hummingbird_ffi_mobile.canSubmitCapture

/** What the rules screen is showing — the same three states every other
 * screen here has, for the same reason: "this device has not synced the
 * rules yet" is a real, reachable, temporary condition, and it is neither
 * a loaded list nor an error. */
sealed interface RulesState {
    data object Loading : RulesState

    data class Loaded(val rules: List<RuleRecord>) : RulesState

    data object NotSynced : RulesState
}

/** One rule being written or edited.
 *
 * `ruleId == null` is a create; anything else is a patch against that row.
 * `eventKind == null` is ADR-0013's "any kind" — a real, chosen value, not
 * an absence, which is why the kind picker always shows it as its own
 * option rather than as an empty state.
 *
 * The conditions are [RuleConditionInput]s verbatim: the seam's own input
 * record, carrying each value as the single string a text field actually
 * holds. Typing it back into the JSON literal the wire carries is decided
 * Rust-side against the field's declared type, so nothing here builds a
 * value and nothing here can get a number-versus-string literal wrong.
 */
data class RuleDraft(
    val ruleId: String?,
    val name: String,
    val eventKind: String?,
    val conditions: List<RuleConditionInput>,
    val severity: String,
    val tier: MobileTier,
    val enabled: Boolean,
    /** The row version this draft was opened over — null on a create, and
     * cleared once [RulesViewModel.save] has refused this draft over it, so
     * that a second Save is the person's own answer to that refusal.
     *
     * A patch from here sends all six fields, and the seam re-reads the
     * newest row for its CAS base — so a rule that moved underneath an open
     * draft would be overwritten at `expected_version`, landing a 200 with
     * no 409 and no rebase. [RulesViewModel.save] compares this against the
     * row it is about to write and refuses the first such save. */
    val baseVersion: Long? = null,
) {
    companion object {
        /** A fresh rule, opened on the form's own defaults — the severity
         * the core decides a fresh rule starts at (never the head of
         * `severities`, which is ADR-0014's ratchet order), and the quieter
         * tier, because a rule that interrupts is a choice a person makes
         * rather than one they are given. */
        fun blank(form: RuleFormRecord) = RuleDraft(
            ruleId = null,
            name = "",
            eventKind = null,
            conditions = emptyList(),
            severity = form.defaultSeverity,
            tier = MobileTier.NORMAL,
            enabled = true,
        )

        /** An existing rule, opened for edit. The conditions come back
         * from the record's already-rendered display values — the same
         * strings the reader was just looking at. */
        fun of(record: RuleRecord) = RuleDraft(
            ruleId = record.id,
            name = record.name,
            eventKind = record.eventKind,
            conditions = record.conditions.map {
                RuleConditionInput(
                    field = it.field,
                    op = it.op,
                    value = it.valueDisplay,
                    negate = it.negate,
                )
            },
            severity = record.severity,
            tier = record.tier,
            enabled = record.enabled,
            baseVersion = record.version,
        )
    }
}

// The rules surface (#540/M4) — the list, the enable/disable toggle, the
// create-and-edit form, and a draft rule's backtest count.
//
// **This class decides nothing about a rule.** Every verdict it renders
// arrives applied from `hummingbird_core::decisions::rules`, through the
// records `ffi-mobile` hands over: `isValid`/`invalidFields` on a
// [RuleRecord], `legalOperators`/`operators`/`durationUnits` on a
// field, `belowAlarmInterval` on a condition row, and `matchCount` on a
// backtest. There is no operator table here, no duration grammar, no
// notion of which fields a kind declares, and no re-derivation of what
// "invalid" means. A Kotlin copy of any of those would be exactly the
// third copy #540 exists to prevent (ADR-0025), and
// `RulesScreenStructuralTest` reads this file to say so.
//
// The injected-fn constructor is the house shape (`NowViewModel`'s own
// doc): a plain JVM test can drive the control flow with no
// host-architecture `.so` in the process.
class RulesViewModel(
    private val fetchRulesFn: suspend () -> List<RuleRecord>,
    private val formFn: suspend (eventKind: String?) -> RuleFormRecord,
    private val createFn: suspend (draft: RuleDraft, nowMs: Long) -> Unit,
    private val patchFn: suspend (draft: RuleDraft, nowMs: Long) -> Unit,
    private val toggleFn: suspend (ruleId: String, enabled: Boolean, nowMs: Long) -> Unit,
    private val deleteFn: suspend (ruleId: String, nowMs: Long) -> Unit,
    private val backtestFn: suspend (
        eventKind: String?,
        conditions: List<RuleConditionInput>,
        nowMs: Long,
    ) -> BacktestRecord,
    private val syncFn: suspend () -> Unit,
    /** The core's blank rule, injected rather than called directly so a
     * plain JVM test can drive this ViewModel — `CaptureViewModel`'s own
     * doc. A hand-written blank check is banned in this repo: the standard
     * library's disagrees with the real rule on a pasted BOM. */
    private val hasContentFn: (String) -> Boolean,
) : ViewModel() {

    private val _state = MutableStateFlow<RulesState>(RulesState.Loading)
    val state: StateFlow<RulesState> = _state.asStateFlow()

    private val _statusLine = MutableStateFlow<String?>(null)
    val statusLine: StateFlow<String?> = _statusLine.asStateFlow()

    /** The switch positions a person has tapped but the authority has not
     * yet handed back, by rule id.
     *
     * `Core::rules()` is deliberately overlay-free — a queued write is
     * invisible to it until a completed cycle pulls the new row back — so a
     * toggle would otherwise render, then revert the instant [load] re-read
     * the unchanged mirror, for up to the 60-second tick above the NavHost.
     * Holding the tapped value here (and saying "pending" beside it) is what
     * keeps the switch from lying about what just happened; the web screen
     * holds its own `pendingEnabled` for exactly this, and this is that. An
     * entry is dropped the moment the re-read row agrees with it. */
    private val _pendingEnabled = MutableStateFlow<Map<String, Boolean>>(emptyMap())
    val pendingEnabled: StateFlow<Map<String, Boolean>> = _pendingEnabled.asStateFlow()

    /** The rule being written or edited, or null in list mode. Held here
     * and never in a `remember {}`: a draft is human-authored content, and
     * a fold or a rotation must not take it (`ScreenStateRetentionTest`). */
    /** The rules a person has deleted but the authority has not yet handed
     * back — see [delete]. Same overlay-free reasoning as [pendingEnabled];
     * an entry is dropped the moment the row stops being listed. */
    private val _pendingDeleted = MutableStateFlow<Set<String>>(emptySet())
    val pendingDeleted: StateFlow<Set<String>> = _pendingDeleted.asStateFlow()

    private val _draft = MutableStateFlow<RuleDraft?>(null)
    val draft: StateFlow<RuleDraft?> = _draft.asStateFlow()

    /** The form for the draft's chosen kind — the fields it offers, each
     * with its legal operators and widget already resolved. Re-read
     * whenever the kind changes, because the field list is a function of
     * the kind and that narrowing is the core's (ADR-0013's "any kind"
     * means the Event core alone). */
    private val _form = MutableStateFlow<RuleFormRecord?>(null)
    val form: StateFlow<RuleFormRecord?> = _form.asStateFlow()

    /** The draft's backtest, or null before one has been run for the
     * current conditions. Never a bare count in the UI — the record
     * carries the corpus caveat beside it. */
    private val _backtest = MutableStateFlow<BacktestRecord?>(null)
    val backtest: StateFlow<BacktestRecord?> = _backtest.asStateFlow()

    /** Whether the draft can be saved at all. A rule needs a name, and
     * whether a string counts as one is the core's answer, never this
     * file's. Conditions may legitimately be empty: a rule that matches
     * every event of its kind is a rule. */
    val canSave: Boolean
        get() = _draft.value?.let { hasContentFn(it.name) } == true

    /** Loads the rules: read, and on an empty read sync once and read
     * again. An open draft is never disturbed — the 60-second cadence
     * above the NavHost ticks while a person is typing. */
    suspend fun load() {
        if (_draft.value == null) _state.value = RulesState.Loading
        try {
            val rules = fetchRulesFn()
            if (rules.isNotEmpty()) {
                _state.value = RulesState.Loaded(rules)
                settlePending(rules)
                _statusLine.value = null
                return
            }
            syncFn()
            val synced = fetchRulesFn()
            _state.value = RulesState.Loaded(synced)
            settlePending(synced)
            _statusLine.value = null
        } catch (error: Exception) {
            _state.value = RulesState.NotSynced
            _statusLine.value = "Couldn't read the rules — ${error.message}"
        }
    }

    /** The enable/disable toggle — one CAS field and nothing else, which
     * is #140's acceptance criterion for it. It is the same seam method
     * every other rule edit uses, so the two can never drift. */
    suspend fun setEnabled(ruleId: String, enabled: Boolean, nowMs: Long) {
        _pendingEnabled.value += (ruleId to enabled)
        try {
            toggleFn(ruleId, enabled, nowMs)
        } catch (error: Exception) {
            _pendingEnabled.value -= ruleId
            _statusLine.value = "Couldn't change that rule — ${error.message}"
            return
        }
        load()
    }

    /** Deletes a rule — a soft delete underneath (`deleted_at`), so the row
     * is flagged rather than erased and every other device learns about it
     * on its ordinary delta pull.
     *
     * Held in [pendingDeleted] the same way, and for the same reason, a
     * toggle is held in [pendingEnabled]: `Core::rules()` has no optimistic
     * overlay, so the re-read below still lists the rule until a cycle
     * lands, and a card that simply stayed put would read as a delete that
     * did not happen. [settlePending] drops the entry the moment the row it
     * names actually leaves the list.
     *
     * The write is not undoable from this screen — the confirm dialog in
     * `RulesScreen.kt` is where that is asked. */
    suspend fun delete(ruleId: String, nowMs: Long) {
        _pendingDeleted.value += ruleId
        try {
            deleteFn(ruleId, nowMs)
        } catch (error: Exception) {
            _pendingDeleted.value -= ruleId
            _statusLine.value = "Couldn't delete that rule — ${error.message}"
            return
        }
        load()
    }

    /** Drops every pending switch position the given rows have caught up
     * with. A row that has not caught up keeps its entry, so the switch
     * holds the tapped value across the reads in between. */
    private fun settlePending(rules: List<RuleRecord>) {
        if (_pendingEnabled.value.isNotEmpty()) {
            _pendingEnabled.value = _pendingEnabled.value.filterNot { (ruleId, enabled) ->
                rules.any { it.id == ruleId && it.enabled == enabled }
            }
        }
        if (_pendingDeleted.value.isNotEmpty()) {
            // A deleted rule leaves the list entirely — the mirror filters
            // it — so "caught up" here is the row being gone, not a field
            // agreeing with what was tapped.
            _pendingDeleted.value =
                _pendingDeleted.value.filterTo(mutableSetOf()) { ruleId ->
                    rules.any { it.id == ruleId }
                }
        }
    }

    suspend fun beginCreate() {
        val form = loadForm(null) ?: return
        _draft.value = RuleDraft.blank(form)
        _backtest.value = null
    }

    suspend fun beginEdit(record: RuleRecord) {
        loadForm(record.eventKind) ?: return
        _draft.value = RuleDraft.of(record)
        _backtest.value = null
    }

    /** Updates the draft. A kind change re-reads the form, because the
     * fields on offer are a function of the kind — and clears the
     * backtest, whose answer was about the previous conditions. */
    suspend fun updateDraft(next: RuleDraft) {
        val previous = _draft.value
        _draft.value = next
        if (previous?.eventKind != next.eventKind) loadForm(next.eventKind)
        if (previous?.conditions != next.conditions || previous.eventKind != next.eventKind) {
            _backtest.value = null
        }
    }

    fun discardEdit() {
        _draft.value = null
        _backtest.value = null
    }

    /** Runs the draft's backtest against this device's own frontier
     * (ADR-0011). A condition the seam refuses — a field the kind does not
     * declare, a value its type cannot hold — is reported rather than
     * silently counted as zero. */
    suspend fun runBacktest(nowMs: Long) {
        val draft = _draft.value ?: return
        _backtest.value = try {
            backtestFn(draft.eventKind, draft.conditions, nowMs)
        } catch (error: Exception) {
            _statusLine.value = "Couldn't backtest — ${error.message}"
            null
        }
    }

    /** Saves the draft — a create or one CAS patch. The draft is cleared
     * only on success: a failed save leaves the words where they can still
     * be seen and retried. */
    suspend fun save(nowMs: Long) {
        val draft = _draft.value ?: return
        if (!canSave) {
            _statusLine.value = "This rule can't be saved yet — a rule needs a name."
            return
        }
        if (draft.ruleId != null && movedUnderTheDraft(draft)) {
            // Refuse once, then let the same tap through: the draft is not
            // reseeded, because that would take words a person is still
            // typing, and it is not silently written either, because the
            // seam's CAS base is the newest row and the other edit would
            // vanish without a 409 anyone could see.
            _draft.value = draft.copy(baseVersion = null)
            _statusLine.value =
                "This rule changed somewhere else while you were editing it. " +
                    "Save again to replace it with what's on screen."
            return
        }
        try {
            if (draft.ruleId == null) createFn(draft, nowMs) else patchFn(draft, nowMs)
            _draft.value = null
            _backtest.value = null
        } catch (error: Exception) {
            _statusLine.value = "Couldn't save — ${error.message}"
            return
        }
        load()
    }

    /** Whether the row this draft is a patch of has moved since it was
     * opened. A read failure answers "no": a save that cannot be checked is
     * still a save the person asked for, and the seam's own CAS is what
     * ultimately decides it. */
    private suspend fun movedUnderTheDraft(draft: RuleDraft): Boolean {
        val base = draft.baseVersion ?: return false
        return try {
            fetchRulesFn().any { it.id == draft.ruleId && it.version != base }
        } catch (error: Exception) {
            false
        }
    }

    private suspend fun loadForm(eventKind: String?): RuleFormRecord? = try {
        formFn(eventKind).also { _form.value = it }
    } catch (error: Exception) {
        _statusLine.value = "Couldn't read the rule form — ${error.message}"
        null
    }

    companion object {
        fun create(context: Context): RulesViewModel {
            suspend fun core() = CoreHolder.get(context.applicationContext)
            return RulesViewModel(
                fetchRulesFn = { core().rules() },
                formFn = { eventKind -> core().ruleForm(eventKind) },
                createFn = { draft, nowMs ->
                    core().createRule(
                        draft.name,
                        draft.eventKind,
                        draft.conditions,
                        draft.severity,
                        draft.tier,
                        draft.enabled,
                        nowMs,
                    )
                },
                patchFn = { draft, nowMs ->
                    core().patchRule(
                        checkNotNull(draft.ruleId) { "a patch draft always carries its id" },
                        draft.name,
                        // "This rule now names no kind" is a real edit and
                        // not the same as silence, which is why the kind
                        // is the one three-way field here.
                        draft.eventKind?.let { FieldPatch.Set(it) } ?: FieldPatch.Clear,
                        draft.conditions,
                        draft.severity,
                        draft.tier,
                        draft.enabled,
                        nowMs,
                    )
                },
                // The toggle is a patch with exactly one field set. Every
                // other argument is left alone, so two devices editing
                // different parts of one rule do not overwrite each other.
                toggleFn = { ruleId, enabled, nowMs ->
                    core().patchRule(
                        ruleId,
                        null,
                        FieldPatch.Untouched,
                        null,
                        null,
                        null,
                        enabled,
                        nowMs,
                    )
                },
                // A soft delete: one flagged column on the same CAS patch
                // the edits use, so the row rides the delta pull off this
                // device rather than vanishing only here.
                deleteFn = { ruleId, nowMs -> core().deleteRule(ruleId, nowMs) },
                backtestFn = { eventKind, conditions, nowMs ->
                    core().backtestRule(
                        eventKind,
                        conditions,
                        // The two frames of one instant — see `WallClock`.
                        WallClock.local(nowMs),
                        WallClock.utc(nowMs),
                    )
                },
                // `"push"`, not `"timer"`: a screen opened during a
                // backoff window must still be able to fetch its rows —
                // `AlertDetailViewModel`'s own note applies verbatim.
                syncFn = {
                    core().run(
                        System.currentTimeMillis(),
                        SyncWorker.TRIGGER_PUSH,
                        false,
                        Random.nextDouble(),
                    )
                },
                hasContentFn = ::canSubmitCapture,
            )
        }

        fun factory(context: Context): ViewModelProvider.Factory = viewModelFactory {
            initializer { create(context) }
        }
    }
}
