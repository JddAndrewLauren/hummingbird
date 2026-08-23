package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.BacktestRecord
import uniffi.hummingbird_ffi_mobile.KindOptionRecord
import uniffi.hummingbird_ffi_mobile.MobileFieldType
import uniffi.hummingbird_ffi_mobile.MobileOperator
import uniffi.hummingbird_ffi_mobile.MobileTier
import uniffi.hummingbird_ffi_mobile.MobileValueWidget
import uniffi.hummingbird_ffi_mobile.RuleConditionInput
import uniffi.hummingbird_ffi_mobile.RuleConditionRecord
import uniffi.hummingbird_ffi_mobile.RuleFieldRecord
import uniffi.hummingbird_ffi_mobile.RuleOperatorRecord
import uniffi.hummingbird_ffi_mobile.RuleFormRecord
import uniffi.hummingbird_ffi_mobile.RuleRecord
import uniffi.hummingbird_ffi_mobile.SourceOptionRecord

// Behavioural, driving the injected fns with fakes — the house shape
// (`NowViewModelTest`, `ItemDetailViewModelTest`). No native library is
// loaded in this process, which is exactly why every seam call this
// ViewModel makes is a constructor argument.
//
// What is *not* tested here, deliberately: any rule decision. Validity,
// operator legality, the widget cascade and the backtest count are
// `hummingbird_core::decisions::rules`' tests to own, and re-asserting them
// against a Kotlin fake would only pin the fake.
class RulesViewModelTest {

    private fun field(
        name: String,
        type: MobileFieldType = MobileFieldType.TEXT,
        operators: List<MobileOperator> = listOf(MobileOperator.EQ, MobileOperator.CONTAINS),
        units: List<String> = emptyList(),
    ) = RuleFieldRecord(
        name = name,
        fieldType = type,
        legalOperators = operators,
        operators = operators.map { RuleOperatorRecord(operator = it, widget = MobileValueWidget.TEXT) },
        durationUnits = units,
    )

    private fun form(vararg fields: RuleFieldRecord) = RuleFormRecord(
        kindOptions = listOf(
            KindOptionRecord(key = null, labelKey = "any_kind"),
            KindOptionRecord(key = "email", labelKey = "email"),
        ),
        fields = fields.toList(),
        severities = listOf("low", "normal", "high", "urgent"),
        sources = listOf(
            SourceOptionRecord(source = "gmail/v1", retiredAs = null),
            SourceOptionRecord(source = "city-waste/v1", retiredAs = "city-waste/v2"),
        ),
        defaultSeverity = "normal",
        tiers = listOf(MobileTier.URGENT, MobileTier.NORMAL),
        alarmIntervalMs = 900_000,
    )

    private fun rule(
        id: String = "r-1",
        enabled: Boolean = true,
        isValid: Boolean = true,
        conditions: List<RuleConditionRecord> = emptyList(),
    ) = RuleRecord(
        id = id,
        name = "passport",
        eventKind = "email",
        kindLabelKey = "email",
        conditions = conditions,
        severity = "high",
        tier = MobileTier.URGENT,
        enabled = enabled,
        isValid = isValid,
        invalidFields = if (isValid) emptyList() else listOf("removed_field"),
        severityIsUnranked = false,
        version = 3,
    )

    private fun conditionRecord(field: String, value: String) = RuleConditionRecord(
        field = field,
        op = MobileOperator.CONTAINS,
        valueDisplay = value,
        negate = false,
        widget = MobileValueWidget.TEXT,
        belowAlarmInterval = false,
    )

    /** A ViewModel with every seam call stubbed. Only what a test cares
     * about is overridden; everything else answers something inert. */
    private fun viewModel(
        rules: () -> List<RuleRecord> = { listOf(rule()) },
        formFor: (String?) -> RuleFormRecord = { form(field("subject")) },
        onCreate: (RuleDraft) -> Unit = {},
        onPatch: (RuleDraft) -> Unit = {},
        onToggle: (String, Boolean) -> Unit = { _, _ -> },
        onDelete: (String) -> Unit = {},
        backtest: (List<RuleConditionInput>) -> BacktestRecord = {
            BacktestRecord(
                isAvailable = true,
                matchCount = 2u,
                corpusNoteKey = "backtest_corpus_frontier_only",
            )
        },
        onSync: () -> Unit = {},
        hasContent: (String) -> Boolean = { it.isNotEmpty() },
    ) = RulesViewModel(
        fetchRulesFn = { rules() },
        formFn = { kind -> formFor(kind) },
        createFn = { draft, _ -> onCreate(draft) },
        patchFn = { draft, _ -> onPatch(draft) },
        toggleFn = { id, enabled, _ -> onToggle(id, enabled) },
        deleteFn = { id, _ -> onDelete(id) },
        backtestFn = { _, conditions, _ -> backtest(conditions) },
        syncFn = { onSync() },
        hasContentFn = hasContent,
    )

    @Test
    fun `loads the rules the seam hands over, in the order given`() = runBlocking {
        val model = viewModel(rules = { listOf(rule("r-2"), rule("r-1")) })
        model.load()
        val loaded = model.state.value as RulesState.Loaded
        assertEquals(listOf("r-2", "r-1"), loaded.rules.map { it.id })
    }

    @Test
    fun `an empty read syncs once and reads again`() = runBlocking {
        var synced = 0
        var reads = 0
        val model = viewModel(
            rules = {
                reads++
                if (reads == 1) emptyList() else listOf(rule())
            },
            onSync = { synced++ },
        )
        model.load()
        assertEquals(1, synced)
        assertEquals(listOf("r-1"), (model.state.value as RulesState.Loaded).rules.map { it.id })
    }

    @Test
    fun `a failed read lands on NotSynced with the reason showing`() = runBlocking {
        val model = viewModel(rules = { error("no mirror") })
        model.load()
        assertEquals(RulesState.NotSynced, model.state.value)
        assertTrue(model.statusLine.value.orEmpty().contains("no mirror"))
    }

    @Test
    fun `the toggle sends exactly one field and re-reads`() = runBlocking {
        val toggles = mutableListOf<Pair<String, Boolean>>()
        val model = viewModel(onToggle = { id, enabled -> toggles += id to enabled })
        model.setEnabled("r-1", false, 1_000)
        assertEquals(listOf("r-1" to false), toggles)
        assertTrue(model.state.value is RulesState.Loaded)
    }

    /** `Core::rules()` has no optimistic overlay, so the re-read after a
     * toggle returns the *unchanged* row until a cycle lands. The tapped
     * position has to survive that, or the switch visibly reverts. */
    @Test
    fun `a toggled switch holds its position until the row catches up`() = runBlocking {
        var landed = false
        val model = viewModel(
            rules = { listOf(rule(enabled = !landed)) },
            onToggle = { _, _ -> },
        )
        model.setEnabled("r-1", false, 1_000)
        assertEquals(mapOf("r-1" to false), model.pendingEnabled.value)

        landed = true
        model.load()
        assertTrue(model.pendingEnabled.value.isEmpty())
    }

    @Test
    fun `a failed toggle drops the pending position rather than lying`() = runBlocking {
        val model = viewModel(onToggle = { _, _ -> error("offline") })
        model.setEnabled("r-1", false, 1_000)
        assertTrue(model.pendingEnabled.value.isEmpty())
    }

    @Test
    fun `a failed toggle reports and does not blank the list`() = runBlocking {
        val model = viewModel(onToggle = { _, _ -> error("offline") })
        model.load()
        model.setEnabled("r-1", false, 1_000)
        assertTrue(model.statusLine.value.orEmpty().contains("offline"))
        assertTrue(model.state.value is RulesState.Loaded)
    }

    @Test
    fun `delete sends the rule id and re-reads`() = runBlocking {
        val deleted = mutableListOf<String>()
        val model = viewModel(onDelete = { deleted += it })
        model.delete("r-1", 1_000)
        assertEquals(listOf("r-1"), deleted)
        assertTrue(model.state.value is RulesState.Loaded)
    }

    /** `Core::rules()` has no optimistic overlay, so the re-read after a
     * delete still lists the row until a cycle lands. The card has to say
     * so, or a delete looks like nothing happened — the same trap the
     * toggle's pending position exists for. */
    @Test
    fun `a deleted rule stays marked pending until it leaves the list`() = runBlocking {
        var landed = false
        val model = viewModel(rules = { if (landed) emptyList() else listOf(rule()) })
        model.delete("r-1", 1_000)
        assertEquals(setOf("r-1"), model.pendingDeleted.value)

        landed = true
        model.load()
        assertTrue(model.pendingDeleted.value.isEmpty())
    }

    @Test
    fun `a failed delete drops the pending mark and reports`() = runBlocking {
        val model = viewModel(onDelete = { error("offline") })
        model.load()
        model.delete("r-1", 1_000)
        assertTrue(model.pendingDeleted.value.isEmpty())
        assertTrue(model.statusLine.value.orEmpty().contains("offline"))
        assertTrue(model.state.value is RulesState.Loaded)
    }

    /** The vocabulary a `source` condition picks from arrives on the form,
     * from the frozen registry — never assembled here. */
    @Test
    fun `the form carries the source vocabulary with retirement marked`() = runBlocking {
        val model = viewModel()
        model.beginCreate()
        val sources = requireNotNull(model.form.value).sources
        assertEquals(listOf("gmail/v1", "city-waste/v1"), sources.map { it.source })
        assertEquals("city-waste/v2", sources.last().retiredAs)
    }

    @Test
    fun `a new draft opens on the form's own defaults`() = runBlocking {
        val model = viewModel()
        model.beginCreate()
        val draft = requireNotNull(model.draft.value)
        assertNull(draft.ruleId)
        assertNull(draft.eventKind)
        // The form's `defaultSeverity`, which the core decides — never the
        // head of `severities`, which is the ratchet order.
        assertEquals("normal", draft.severity)
        assertEquals(MobileTier.NORMAL, draft.tier)
        assertTrue(draft.conditions.isEmpty())
    }

    @Test
    fun `editing a rule carries its id, so the save is a patch`() = runBlocking {
        var created = 0
        var patched = 0
        val model = viewModel(onCreate = { created++ }, onPatch = { patched++ })
        model.beginEdit(rule(conditions = listOf(conditionRecord("subject", "passport"))))
        val draft = requireNotNull(model.draft.value)
        assertEquals("r-1", draft.ruleId)
        assertEquals(listOf("passport"), draft.conditions.map { it.value })
        model.save(1_000)
        assertEquals(0, created)
        assertEquals(1, patched)
    }

    /** A patch from here sends every field and the seam takes the *newest*
     * row as its CAS base, so a rule that moved under an open draft would
     * be overwritten at a version the authority accepts — a 200 with no 409
     * and nothing in the conflict journal. Refuse the first such save. */
    @Test
    fun `a rule that moved under an open draft is not silently overwritten`() = runBlocking {
        var patched = 0
        var version = 3L
        val model = viewModel(
            rules = { listOf(rule().copy(version = version)) },
            onPatch = { patched++ },
        )
        model.beginEdit(rule())

        version = 4
        model.save(1_000)
        assertEquals(0, patched)
        assertTrue(model.statusLine.value.orEmpty().contains("changed somewhere else"))
        // The words are still on screen — a refusal, not a discard.
        assertEquals("passport", requireNotNull(model.draft.value).name)

        // Saving again is the person's own answer to that refusal.
        model.save(1_000)
        assertEquals(1, patched)
    }

    @Test
    fun `a draft with no id saves as a create`() = runBlocking {
        var created = 0
        val model = viewModel(onCreate = { created++ })
        model.beginCreate()
        model.updateDraft(requireNotNull(model.draft.value).copy(name = "watch for passports"))
        model.save(1_000)
        assertEquals(1, created)
        assertNull(model.draft.value)
    }

    @Test
    fun `a nameless draft is refused rather than saved as a silent no-op`() = runBlocking {
        var created = 0
        val model = viewModel(onCreate = { created++ })
        model.beginCreate()
        assertFalse(model.canSave)
        model.save(1_000)
        assertEquals(0, created)
        // The draft survives the refusal — the words stay where they can
        // still be seen.
        assertTrue(model.draft.value != null)
        assertTrue(model.statusLine.value.orEmpty().contains("name"))
    }

    @Test
    fun `whether a name counts as one is the injected rule, never Kotlin's`() = runBlocking {
        // The fake refuses everything, so a draft with a real-looking name
        // is still unsaveable — proving the ViewModel asks rather than
        // deciding.
        val model = viewModel(hasContent = { false })
        model.beginCreate()
        model.updateDraft(requireNotNull(model.draft.value).copy(name = "watch for passports"))
        assertFalse(model.canSave)
    }

    @Test
    fun `a failed save keeps the draft`() = runBlocking {
        val model = viewModel(onCreate = { error("409") })
        model.beginCreate()
        model.updateDraft(requireNotNull(model.draft.value).copy(name = "n"))
        model.save(1_000)
        assertTrue(model.draft.value != null)
        assertTrue(model.statusLine.value.orEmpty().contains("409"))
    }

    @Test
    fun `changing the kind re-reads the form, because the fields follow the kind`() = runBlocking {
        val asked = mutableListOf<String?>()
        val model = viewModel(
            formFor = { kind ->
                asked += kind
                if (kind == null) form(field("source")) else form(field("subject"))
            },
        )
        model.beginCreate()
        model.updateDraft(requireNotNull(model.draft.value).copy(eventKind = "email"))
        assertEquals(listOf(null, "email"), asked)
        assertEquals(listOf("subject"), model.form.value?.fields?.map { it.name })
    }

    @Test
    fun `a backtest answer is dropped when the conditions it was about change`() = runBlocking {
        val model = viewModel()
        model.beginCreate()
        model.runBacktest(1_000)
        assertEquals(2u, model.backtest.value?.matchCount)
        model.updateDraft(
            requireNotNull(model.draft.value).copy(
                conditions = listOf(
                    RuleConditionInput("subject", MobileOperator.CONTAINS, "passport", false),
                ),
            ),
        )
        assertNull(model.backtest.value)
    }

    @Test
    fun `a refused backtest reports rather than counting zero`() = runBlocking {
        val model = viewModel(backtest = { error("not a field this kind declares") })
        model.beginCreate()
        model.runBacktest(1_000)
        assertNull(model.backtest.value)
        assertTrue(model.statusLine.value.orEmpty().contains("not a field"))
    }

    @Test
    fun `discarding clears the draft and the answer that was about it`() = runBlocking {
        val model = viewModel()
        model.beginCreate()
        model.runBacktest(1_000)
        model.discardEdit()
        assertNull(model.draft.value)
        assertNull(model.backtest.value)
    }

    @Test
    fun `a reload never disturbs an open draft`() = runBlocking {
        val model = viewModel()
        model.beginCreate()
        model.updateDraft(requireNotNull(model.draft.value).copy(name = "half typed"))
        model.load()
        assertEquals("half typed", model.draft.value?.name)
    }
}
