package net.twinion.hummingbird

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.emptyPreferences
import java.io.IOException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileCalendarSelection

// `CalendarPrefs` (#564) is `FrontierPrefs`/`PanePrefs`' sibling and
// inherits their rules verbatim — a failed read is the default, a failed
// write is silence — so these are those files' tests re-aimed through the
// same `internal` store-taking overloads.
//
// What is this file's own: the **absence-is-never-connected** convention,
// the two-set representation of the selection (a calendar id is opaque
// provider text, so no separator is safe to encode with), and the one
// invariant that matters more than any of it — **nothing token-shaped is
// stored here**, checked as a source gate below because a stored
// credential compiles and passes every behavioural test anyone would
// write.
class CalendarPrefsTest {

    private class FailingStore : DataStore<Preferences> {
        override val data: Flow<Preferences> = flow { throw IOException("unreadable") }

        override suspend fun updateData(
            transform: suspend (Preferences) -> Preferences,
        ): Preferences = throw IOException("unwritable")
    }

    private class WorkingStore : DataStore<Preferences> {
        var stored: Preferences = emptyPreferences()

        override val data: Flow<Preferences> = flow { emit(stored) }

        override suspend fun updateData(
            transform: suspend (Preferences) -> Preferences,
        ): Preferences = transform(stored).also { stored = it }
    }

    @Test
    fun `a fresh store has never connected`() = runBlocking {
        assertFalse(CalendarPrefs.readConnected(WorkingStore()))
    }

    @Test
    fun `the connected flag round-trips and disconnecting removes it`() = runBlocking {
        val store = WorkingStore()

        CalendarPrefs.writeConnected(store, true)
        assertTrue(CalendarPrefs.readConnected(store))

        CalendarPrefs.writeConnected(store, false)
        assertFalse(CalendarPrefs.readConnected(store))
        // Absence, not a stored `false` — the same convention
        // `FrontierPrefs` uses for its default axis.
        assertTrue(store.stored.asMap().isEmpty())
    }

    @Test
    fun `selections round-trip with their horizons`() = runBlocking {
        val store = WorkingStore()
        val selections = listOf(
            MobileCalendarSelection(id = "primary", longHorizon = false),
            MobileCalendarSelection(id = "trips@group.calendar.google.com", longHorizon = true),
        )

        CalendarPrefs.writeSelections(store, selections)

        assertEquals(selections.sortedBy { it.id }, CalendarPrefs.readSelections(store))
    }

    @Test
    fun `a calendar id containing every plausible separator survives`() = runBlocking {
        // The reason the selection is two sets rather than one encoded
        // string: a provider id is opaque text, and any separator a
        // hand-rolled encoding picked could appear inside one.
        val store = WorkingStore()
        val awkward = MobileCalendarSelection(id = "a,b;c|d\te\nf#g", longHorizon = true)

        CalendarPrefs.writeSelections(store, listOf(awkward))

        assertEquals(listOf(awkward), CalendarPrefs.readSelections(store))
    }

    @Test
    fun `an empty selection removes both keys rather than storing emptiness`() = runBlocking {
        val store = WorkingStore()
        CalendarPrefs.writeSelections(
            store,
            listOf(MobileCalendarSelection(id = "primary", longHorizon = true)),
        )

        CalendarPrefs.writeSelections(store, emptyList())

        assertEquals(emptyList<MobileCalendarSelection>(), CalendarPrefs.readSelections(store))
        assertTrue(store.stored.asMap().isEmpty())
    }

    @Test
    fun `a store that cannot be read answers the defaults rather than throwing`() = runBlocking {
        assertFalse(CalendarPrefs.readConnected(FailingStore()))
        assertEquals(emptyList<MobileCalendarSelection>(), CalendarPrefs.readSelections(FailingStore()))
    }

    @Test
    fun `a store that cannot be written fails silently`() = runBlocking {
        // A preference that cannot persist still applies for the session;
        // there is nothing to report and nothing to retry.
        CalendarPrefs.writeConnected(FailingStore(), true)
        CalendarPrefs.writeSelections(
            FailingStore(),
            listOf(MobileCalendarSelection(id = "primary", longHorizon = false)),
        )
    }

    @Test
    fun `nothing token-shaped is stored in the calendar preferences`() {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val source = java.io.File(
            root,
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/CalendarPrefs.kt",
        ).readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")

        // The Google access token is minted and held in Rust and never
        // crosses the seam; the device token is `TokenStore`'s. A
        // preferences key naming either would be a second credential
        // lifecycle, and it would look perfectly ordinary in a diff.
        for (word in listOf("token", "Token", "accessToken", "credential")) {
            assertFalse(
                "CalendarPrefs.kt must store no credential — found $word",
                source.contains(word),
            )
        }
    }
}
