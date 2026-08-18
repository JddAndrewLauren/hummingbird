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

// Behavioural, driving the injected fns with fakes — `RulesViewModelTest`'s
// own house shape. What is *not* tested here, deliberately: the dead-letter
// heading's pluralisation — `hummingbird_core::decisions::settings`'s own
// test to own, and re-asserting it against a fake would only pin the fake.
// The sync card's `lastSyncOutcomeKind`/`lastSyncAtMs`/`syncStatusSummary`
// moved out of this class entirely (round-1 review, #535) — see
// `SettingsScreen.kt`'s own doc for why, and `AppRoot`'s own cadence in
// `MainActivity.kt` for where that state and its update now live.
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
        )

        viewModel.setBinding("race-series", "motogp", 1_000)

        assertEquals("race-series" to "disk full", viewModel.bindingError.value)
    }

    @Test
    fun `the dead-letter heading is off the real count, off an injected fn never the native one`() = runBlocking {
        val viewModel = SettingsViewModel(
            fetchFn = {
                SettingsRead(
                    bindings = emptyList(),
                    deadLetters = listOf(deadLetter("q-1"), deadLetter("q-2")),
                    queueDepth = 0u,
                )
            },
            setBindingFn = { _, _, _ -> },
            deadLetterHeadingFn = { count -> "fake heading for $count" },
        )

        viewModel.load()

        assertEquals("fake heading for 2", viewModel.deadLetterHeadingText())
    }
}
