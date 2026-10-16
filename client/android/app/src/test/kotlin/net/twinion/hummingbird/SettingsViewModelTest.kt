package net.twinion.hummingbird

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileBindingRecord
import uniffi.hummingbird_ffi_mobile.MobileBindingValue
import uniffi.hummingbird_ffi_mobile.MobileDeadLetterReason
import uniffi.hummingbird_ffi_mobile.MobileDeadLetterRecord
import uniffi.hummingbird_ffi_mobile.MobileDeviceInfo
import uniffi.hummingbird_ffi_mobile.MobileDeviceListPlatform
import uniffi.hummingbird_ffi_mobile.MobileDeviceRow
import uniffi.hummingbird_ffi_mobile.MobileDevicesOutcome
import uniffi.hummingbird_ffi_mobile.MobileQuestionRosterEntry
import uniffi.hummingbird_ffi_mobile.MobileQuestionSwitch
import uniffi.hummingbird_ffi_mobile.MobileRevokeOutcome
import uniffi.hummingbird_ffi_mobile.MobileSetBindingException
import uniffi.hummingbird_ffi_mobile.MobileSignInOutcome
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.MobileSurface
import net.twinion.hummingbird.signin.GoogleCredential

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

    @Test
    fun `exportDiagnostics delegates to the injected fn and returns its bytes`() = runBlocking {
        val exported = byteArrayOf(1, 2, 3)
        val viewModel = SettingsViewModel(
            fetchFn = { SettingsRead(bindings = emptyList(), deadLetters = emptyList(), queueDepth = 0u) },
            setBindingFn = { _, _, _ -> },
            exportDiagnosticsFn = { exported },
        )

        assertTrue(exported.contentEquals(viewModel.exportDiagnostics()))
    }

    @Test
    fun `clearDiagnostics delegates to the injected fn`() = runBlocking {
        var cleared = false
        val viewModel = SettingsViewModel(
            fetchFn = { SettingsRead(bindings = emptyList(), deadLetters = emptyList(), queueDepth = 0u) },
            setBindingFn = { _, _, _ -> },
            clearDiagnosticsFn = { cleared = true },
        )

        viewModel.clearDiagnostics()

        assertTrue(cleared)
    }

    // -- the standing-question roster (#716) ------------------------------
    //
    // The roster here is a fake with made-up words on purpose: its labels,
    // order and bindings are the core's, and the claim under test is that
    // this class renders whatever the seam hands it, in the seam's order —
    // never that it agrees with any particular wording.

    private val roster = listOf(
        MobileQuestionRosterEntry(MobileStandingQuestion.WASTE, "Third word", MobileSurface.NOW, listOf("key-w")),
        MobileQuestionRosterEntry(MobileStandingQuestion.HOMEWORK, "First word", MobileSurface.NOW, listOf("key-h")),
        MobileQuestionRosterEntry(MobileStandingQuestion.KIMI, "Second word", MobileSurface.STATUS, emptyList()),
    )

    /** A stand-in for the core's `settings` mirror with its overlay: a
     * write lands as `pending`, and [confirm] is a sync cycle acknowledging
     * it. One instance outliving two view models is a process death. */
    private class FakeSwitchStore {
        val switches = roster().associateWith { MobileQuestionSwitch(it, enabled = true, pending = false) }.toMutableMap()

        fun set(question: MobileStandingQuestion, enabled: Boolean) {
            switches[question] = MobileQuestionSwitch(question, enabled, pending = true)
        }

        fun confirm() {
            switches.replaceAll { _, switch -> switch.copy(pending = false) }
        }

        fun read() = switches.values.toList()

        companion object {
            fun roster() = listOf(MobileStandingQuestion.WASTE, MobileStandingQuestion.HOMEWORK, MobileStandingQuestion.KIMI)
        }
    }

    private fun rosterViewModel(
        bindings: List<MobileBindingRecord>,
        store: FakeSwitchStore = FakeSwitchStore(),
        setQuestionEnabledFn: suspend (MobileStandingQuestion, Boolean, Long) -> Unit =
            { question, enabled, _ -> store.set(question, enabled) },
    ) = SettingsViewModel(
        fetchFn = { SettingsRead(bindings = bindings, deadLetters = emptyList(), queueDepth = 0u, switches = store.read()) },
        setBindingFn = { _, _, _ -> },
        rosterFn = { roster },
        setQuestionEnabledFn = setQuestionEnabledFn,
    )

    @Test
    fun `every question the seam reports renders in the seam's order with its bindings nested`() = runBlocking {
        val viewModel = rosterViewModel(listOf(binding("key-h"), binding("key-w")))

        viewModel.load()

        val groups = viewModel.questions.value!!.groups
        assertEquals(
            listOf(MobileStandingQuestion.WASTE, MobileStandingQuestion.HOMEWORK, MobileStandingQuestion.KIMI),
            groups.map { it.question },
        )
        assertEquals(listOf("Third word", "First word", "Second word"), groups.map { it.label })
        assertEquals(listOf("key-w"), groups[0].rows.map { it.key })
        assertEquals(listOf("key-h"), groups[1].rows.map { it.key })
        assertTrue("a question with no binding still renders, with nothing nested", groups[2].rows.isEmpty())
        assertEquals(listOf(true, true, true), groups.map { it.enabled })
    }

    @Test
    fun `unknown settings rows land under other settings rows, never dropped`() = runBlocking {
        val viewModel = rosterViewModel(
            listOf(binding("key-h"), binding("mystery-key", known = false), binding("unclaimed-known")),
        )

        viewModel.load()

        val grouped = viewModel.questions.value!!
        assertEquals(listOf("mystery-key", "unclaimed-known"), grouped.other.map { it.key })
        val total = grouped.groups.sumOf { it.rows.size } + grouped.other.size
        assertEquals("every row lands in exactly one place", 3, total)
    }

    @Test
    fun `an empty mirror still renders every question, saying which rows are missing`() = runBlocking {
        val viewModel = rosterViewModel(bindings = emptyList())

        viewModel.load()

        val grouped = viewModel.questions.value!!
        assertEquals(3, grouped.groups.size)
        assertTrue(grouped.other.isEmpty())
        assertEquals(listOf("key-w"), grouped.groups[0].missing)
        assertTrue(grouped.groups[2].missing.isEmpty())
    }

    @Test
    fun `a question with no switch handed over draws no toggle rather than a guessed one`() {
        val grouped = groupBindingsByQuestion(roster, emptyList(), switches = emptyList())

        assertEquals(listOf(null, null, null), grouped.groups.map { it.enabled })
        assertEquals(listOf(false, false, false), grouped.groups.map { it.pending })
    }

    @Test
    fun `the toggle writes through the seam and survives a sync cycle and a process death`() = runBlocking {
        val store = FakeSwitchStore()
        val writes = mutableListOf<Pair<MobileStandingQuestion, Boolean>>()
        val viewModel = rosterViewModel(emptyList(), store) { question, enabled, _ ->
            writes += question to enabled
            store.set(question, enabled)
        }
        viewModel.load()

        viewModel.setQuestionEnabled(MobileStandingQuestion.HOMEWORK, false, 1_000)

        assertEquals(listOf(MobileStandingQuestion.HOMEWORK to false), writes)
        val written = viewModel.questions.value!!.groups.single { it.question == MobileStandingQuestion.HOMEWORK }
        assertEquals(false, written.enabled)
        assertTrue("the overlaid write reads as queued", written.pending)

        // A sync cycle acknowledges it; the reload after it keeps the state.
        store.confirm()
        viewModel.load()
        val synced = viewModel.questions.value!!.groups.single { it.question == MobileStandingQuestion.HOMEWORK }
        assertEquals(false, synced.enabled)
        assertFalse(synced.pending)

        // A fresh view model over the same store is a process death.
        val reborn = rosterViewModel(emptyList(), store)
        reborn.load()
        assertEquals(
            false,
            reborn.questions.value!!.groups.single { it.question == MobileStandingQuestion.HOMEWORK }.enabled,
        )
        assertNull(viewModel.questionSwitchError.value)
    }

    @Test
    fun `a failed toggle is reported on its own question, never another`() = runBlocking {
        val viewModel = rosterViewModel(emptyList()) { _, _, _ ->
            throw MobileSetBindingException.WriteFailed("disk full")
        }

        viewModel.setQuestionEnabled(MobileStandingQuestion.KIMI, false, 1_000)

        assertEquals(MobileStandingQuestion.KIMI to "disk full", viewModel.questionSwitchError.value)
    }

    // -- sign-in and Signed-in devices (#841) -----------------------------

    private fun emptyRead() = SettingsRead(bindings = emptyList(), deadLetters = emptyList(), queueDepth = 0u)

    @Test
    fun `a sign-in that mints a token saves it through the callback it was handed, and says nothing`() =
        runBlocking {
            val saved = mutableListOf<String>()
            val viewModel = SettingsViewModel(
                fetchFn = { emptyRead() },
                setBindingFn = { _, _, _ -> },
                signInFn = { _, _ -> MobileSignInOutcome.SignedIn(id = "device-android-pixel", token = "hb_minted") },
            )

            viewModel.signIn({ GoogleCredential.IdToken("jwt") }, "Pixel") { token -> saved += token }

            assertEquals(listOf("hb_minted"), saved)
            assertNull(viewModel.signInNotice.value)
            assertFalse(viewModel.signingIn.value)
        }

    @Test
    fun `a refused sign-in leaves its sentence and saves nothing`() = runBlocking {
        val saved = mutableListOf<String>()
        val viewModel = SettingsViewModel(
            fetchFn = { emptyRead() },
            setBindingFn = { _, _, _ -> },
            signInFn = { _, _ -> MobileSignInOutcome.RateLimited },
        )

        viewModel.signIn({ GoogleCredential.IdToken("jwt") }, "Pixel") { token -> saved += token }

        assertTrue(saved.isEmpty())
        assertNotNull(viewModel.signInNotice.value)
        assertFalse(viewModel.signingIn.value)
    }

    private fun deviceInfo(id: String, lastSeen: Long?, isCurrent: Boolean, revocable: Boolean = true) =
        MobileDeviceInfo(
            id = id,
            name = "$id name",
            platform = MobileDeviceListPlatform.OTHER,
            createdAt = 0,
            lastSeen = lastSeen,
            revokedAt = null,
            isCurrent = isCurrent,
            revocable = revocable,
        )

    /** The real core decision (`deviceRows`, through the host cdylib), not a
     * fake: the claim is that this device leads whatever order the
     * authority answered in, and that `revocable` arrives as the route said. */
    @Test
    fun `the devices section lists this device first and carries each row's revocable verdict`() = runBlocking {
        val now = 100L * 3_600_000
        val viewModel = SettingsViewModel(
            fetchFn = { emptyRead() },
            setBindingFn = { _, _, _ -> },
            listDevicesFn = {
                MobileDevicesOutcome.Listed(
                    listOf(
                        deviceInfo("runner", lastSeen = now, isCurrent = false, revocable = false),
                        deviceInfo("device-web-laptop", lastSeen = now - 60_000, isCurrent = false),
                        deviceInfo("device-android-pixel", lastSeen = now - 3_600_000, isCurrent = true),
                    ),
                )
            },
            nowMsFn = { now },
        )

        viewModel.loadDevices()

        val rows = (viewModel.devices.value as DevicesState.Listed).rows
        assertEquals("device-android-pixel", rows.first().id)
        assertTrue(rows.first().isCurrent)
        assertNotNull("this device's row is labelled as such", rows.first().thisDevice)
        assertEquals(false, rows.single { it.id == "runner" }.revocable)
    }

    @Test
    fun `no credential reads as a sentence, not an empty list`() = runBlocking {
        val viewModel = SettingsViewModel(
            fetchFn = { emptyRead() },
            setBindingFn = { _, _, _ -> },
            listDevicesFn = { MobileDevicesOutcome.NoCredential },
        )

        viewModel.loadDevices()

        assertTrue(viewModel.devices.value is DevicesState.Unavailable)
    }

    private fun row(id: String, isCurrent: Boolean) = MobileDeviceRow(
        id = id,
        name = "$id name",
        isCurrent = isCurrent,
        thisDevice = if (isCurrent) "This device" else null,
        platformLabel = "Android",
        lastSeenLabel = "Last seen just now",
        revocable = true,
    )

    @Test
    fun `this device's confirmation is the core's, off the real queue depth`() = runBlocking {
        val viewModel = SettingsViewModel(
            fetchFn = { SettingsRead(bindings = emptyList(), deadLetters = emptyList(), queueDepth = 3u) },
            setBindingFn = { _, _, _ -> },
            signOutConfirmationFn = { depth -> "core sentence for $depth" },
        )
        viewModel.load()

        assertEquals("core sentence for 3", viewModel.signOutConfirmationText(row("me", isCurrent = true)))
    }

    /** #887: another device's confirmation is the core's too — the real
     * seam call, so the sentence asserted is the one the core words. */
    @Test
    fun `another device's confirmation is the core's sentence`() {
        val viewModel = SettingsViewModel(fetchFn = { emptyRead() }, setBindingFn = { _, _, _ -> })

        assertEquals(
            "Sign out other name? It stops syncing until it signs in again.",
            viewModel.signOutConfirmationText(row("other", isCurrent = false)),
        )
    }

    @Test
    fun `signing this device out forgets the stored token, another device does not`() = runBlocking {
        var forgotten = 0
        var answer: MobileRevokeOutcome = MobileRevokeOutcome.SignedOut(deviceInfo("me", null, true))
        val viewModel = SettingsViewModel(
            fetchFn = { emptyRead() },
            setBindingFn = { _, _, _ -> },
            revokeDeviceFn = { answer },
        )

        viewModel.signOut(row("me", isCurrent = true)) { forgotten += 1 }
        assertEquals(1, forgotten)

        answer = MobileRevokeOutcome.Revoked(deviceInfo("other", null, false))
        viewModel.signOut(row("other", isCurrent = false)) { forgotten += 1 }
        assertEquals("revoking another device leaves this one's token", 1, forgotten)
        assertNull(viewModel.deviceError.value)
    }

    @Test
    fun `a refused sign-out shows on its own row and forgets nothing`() = runBlocking {
        var forgotten = false
        val viewModel = SettingsViewModel(
            fetchFn = { emptyRead() },
            setBindingFn = { _, _, _ -> },
            revokeDeviceFn = { MobileRevokeOutcome.NotRevocable },
        )

        viewModel.signOut(row("runner", isCurrent = false)) { forgotten = true }

        assertFalse(forgotten)
        assertEquals("runner", viewModel.deviceError.value?.first)
    }
}
