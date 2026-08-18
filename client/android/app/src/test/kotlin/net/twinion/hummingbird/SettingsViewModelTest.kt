package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileBindingRecord
import uniffi.hummingbird_ffi_mobile.MobileBindingValue
import uniffi.hummingbird_ffi_mobile.MobileDeadLetterReason
import uniffi.hummingbird_ffi_mobile.MobileDeadLetterRecord
import uniffi.hummingbird_ffi_mobile.MobileSetBindingException
import uniffi.hummingbird_ffi_mobile.MobileSyncStatusSummary
import uniffi.hummingbird_ffi_mobile.MobileSyncStatusTone

// Behavioural, driving the injected fns with fakes — `RulesViewModelTest`'s
// own house shape. What is *not* tested here, deliberately: the sync-status
// summary's tone/label/word or the dead-letter heading's pluralisation —
// both are `hummingbird_core::decisions::settings`' own tests to own, and
// re-asserting them against a fake would only pin the fake.
class SettingsViewModelTest {

    private fun binding(key: String, known: Boolean = true) = MobileBindingRecord(
        key = key,
        known = known,
        pending = false,
        value = MobileBindingValue.Unset,
    )

    private fun deadLetter(id: String) = MobileDeadLetterRecord(
        id = id,
        reason = MobileDeadLetterReason.Permanent("rejected"),
        fields = emptyList(),
        atMs = 0,
        entity = "settings",
        entityId = null,
    )

    @Test
    fun `load reads bindings, dead letters and queue depth together`() = runBlocking {
        val viewModel = SettingsViewModel(
            fetchFn = {
                SettingsRead(
                    bindings = listOf(binding("race-series")),
                    deadLetters = listOf(deadLetter("q-1")),
                    queueDepth = 2u,
                )
            },
            setBindingFn = { _, _, _ -> },
            runFn = { "completed" },
            onlineFn = { true },
        )

        viewModel.load()

        assertEquals(listOf(binding("race-series")), viewModel.bindings.value)
        assertEquals(1, viewModel.deadLetters.value.size)
        assertEquals(2u, viewModel.queueDepth.value)
    }

    @Test
    fun `a successful binding write clears any previous error for that key and reloads`() = runBlocking {
        var writes = 0
        val viewModel = SettingsViewModel(
            fetchFn = {
                writes += 1
                SettingsRead(bindings = emptyList(), deadLetters = emptyList(), queueDepth = 0u)
            },
            setBindingFn = { _, _, _ -> },
            runFn = { "completed" },
            onlineFn = { true },
        )

        viewModel.setBinding("race-series", "motogp", 1_000)

        assertNull(viewModel.bindingError.value)
        assertTrue("load ran after a successful write", writes >= 1)
    }

    @Test
    fun `an unknown-key rejection is reported on that rows own key, never a wrong one`() = runBlocking {
        val viewModel = SettingsViewModel(
            fetchFn = { SettingsRead(bindings = emptyList(), deadLetters = emptyList(), queueDepth = 0u) },
            setBindingFn = { _, _, _ -> throw MobileSetBindingException.UnknownKey() },
            runFn = { "completed" },
            onlineFn = { true },
        )

        viewModel.setBinding("mystery-key", "value", 1_000)

        val error = viewModel.bindingError.value
        assertEquals("mystery-key", error?.first)
        assertTrue(error?.second?.contains("doesn't know that binding") == true)
    }

    @Test
    fun `a write failure is reported with the core's own detail`() = runBlocking {
        val viewModel = SettingsViewModel(
            fetchFn = { SettingsRead(bindings = emptyList(), deadLetters = emptyList(), queueDepth = 0u) },
            setBindingFn = { _, _, _ -> throw MobileSetBindingException.WriteFailed("disk full") },
            runFn = { "completed" },
            onlineFn = { true },
        )

        viewModel.setBinding("race-series", "motogp", 1_000)

        assertEquals("race-series" to "disk full", viewModel.bindingError.value)
    }

    // The two tests below drive `syncSummary`/`sync`, which in production
    // reach the real `hummingbird_core::decisions::settings::
    // sync_status_summary`/`is_informative_sync_outcome` (#535). Both are
    // injected here instead — a fake that just echoes its input back —
    // for the same "no native library in this process" reason
    // `RulesViewModelTest`'s header states: what the core actually decides
    // is that module's own test to own, and this class's job is only to
    // feed it the right `lastSyncOutcomeKind`/`lastSyncAtMs` off the right
    // sync attempt.
    private fun echoSummary(input: uniffi.hummingbird_ffi_mobile.MobileSyncStatusInput) = MobileSyncStatusSummary(
        tone = MobileSyncStatusTone.NEUTRAL,
        label = "kind=${input.lastSyncOutcomeKind} atMs=${input.lastSyncAtMs}",
        toneWord = "fake",
    )

    @Test
    fun `sync records only an informative outcomes kind and timestamp`() = runBlocking {
        val viewModel = SettingsViewModel(
            fetchFn = { SettingsRead(bindings = emptyList(), deadLetters = emptyList(), queueDepth = 0u) },
            setBindingFn = { _, _, _ -> },
            runFn = { "skipped" },
            onlineFn = { true },
            syncStatusSummaryFn = ::echoSummary,
            isInformativeSyncOutcomeFn = { kind -> kind != "skipped" && kind != "busy" },
        )

        viewModel.sync(60_000)

        // A "skipped" outcome is not informative — the sync card must keep
        // whatever it showed before (nothing, here), never read the
        // skipped tick as fresh news.
        val summary = viewModel.syncSummary(120_000)
        assertEquals("kind=null atMs=null", summary.label)
    }

    @Test
    fun `sync records an informative outcomes kind and timestamp`() = runBlocking {
        val viewModel = SettingsViewModel(
            fetchFn = { SettingsRead(bindings = emptyList(), deadLetters = emptyList(), queueDepth = 0u) },
            setBindingFn = { _, _, _ -> },
            runFn = { "completed" },
            onlineFn = { true },
            syncStatusSummaryFn = ::echoSummary,
            isInformativeSyncOutcomeFn = { kind -> kind != "skipped" && kind != "busy" },
        )

        viewModel.sync(1_000)

        val summary = viewModel.syncSummary(120_000)
        assertEquals("kind=completed atMs=1000", summary.label)
    }
}
