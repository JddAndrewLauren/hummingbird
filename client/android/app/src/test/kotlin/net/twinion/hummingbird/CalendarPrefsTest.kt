package net.twinion.hummingbird

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.mutablePreferencesOf
import androidx.datastore.preferences.core.stringSetPreferencesKey
import java.io.IOException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileCalendarSelection

// `CalendarPrefs` (#564) is `FrontierPrefs`/`PanePrefs`' sibling and
// inherits their rules verbatim — a failed read is the default, a failed
// write is silence — so these are those files' tests re-aimed through the
// same `internal` store-taking overloads. Since #843 (ADR-0040 decision 8)
// the store only ever feeds the core's one-time seed, so what is tested is
// reading the pre-#843 copy back and clearing it.
//
// The one invariant that matters more than any of it — **nothing
// token-shaped is stored here** — is checked as a source gate below,
// because a stored credential compiles and passes every behavioural test
// anyone would write.
class CalendarPrefsTest {

    private class FailingStore : DataStore<Preferences> {
        override val data: Flow<Preferences> = flow { throw IOException("unreadable") }

        override suspend fun updateData(
            transform: suspend (Preferences) -> Preferences,
        ): Preferences = throw IOException("unwritable")
    }

    private class WorkingStore(initial: Preferences = emptyPreferences()) : DataStore<Preferences> {
        var stored: Preferences = initial

        override val data: Flow<Preferences> = flow { emit(stored) }

        override suspend fun updateData(
            transform: suspend (Preferences) -> Preferences,
        ): Preferences = transform(stored).also { stored = it }
    }

    /** The pre-#843 shape exactly as #564's `writeSelections` laid it down. */
    private fun oldCopy(connected: Boolean?, selected: Set<String>?, long: Set<String>?) =
        mutablePreferencesOf().apply {
            connected?.let { this[booleanPreferencesKey("connected")] = it }
            selected?.let { this[stringSetPreferencesKey("selected_calendars")] = it }
            long?.let { this[stringSetPreferencesKey("long_horizon_calendars")] = it }
        }

    @Test
    fun `a device that never connected has nothing to migrate`() = runBlocking {
        assertNull(CalendarPrefs.readLocal(WorkingStore()))
    }

    @Test
    fun `the old copy reads back with its horizons`() = runBlocking {
        val store = WorkingStore(
            oldCopy(
                connected = true,
                selected = setOf("trips@group.calendar.google.com", "primary"),
                long = setOf("trips@group.calendar.google.com"),
            ),
        )

        assertEquals(
            CalendarPrefs.Local(
                connected = true,
                selections = listOf(
                    MobileCalendarSelection(id = "primary", longHorizon = false),
                    MobileCalendarSelection(id = "trips@group.calendar.google.com", longHorizon = true),
                ),
            ),
            CalendarPrefs.readLocal(store),
        )
    }

    @Test
    fun `picked calendars without the flag read as not connected`() = runBlocking {
        val local = CalendarPrefs.readLocal(WorkingStore(oldCopy(null, setOf("primary"), null)))
        assertNotNull(local)
        assertFalse(local!!.connected)
    }

    @Test
    fun `clearing removes every key, so nothing is migrated twice`() = runBlocking {
        val store = WorkingStore(oldCopy(true, setOf("primary"), setOf("primary")))

        CalendarPrefs.clear(store)

        assertTrue(store.stored.asMap().isEmpty())
        assertNull(CalendarPrefs.readLocal(store))
    }

    @Test
    fun `a store that cannot be read or written never throws`() = runBlocking {
        assertNull(CalendarPrefs.readLocal(FailingStore()))
        CalendarPrefs.clear(FailingStore())
    }

    @Test
    fun `nothing token-shaped is stored in the calendar preferences`() {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
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
