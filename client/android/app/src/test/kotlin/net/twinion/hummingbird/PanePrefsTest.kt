package net.twinion.hummingbird

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.emptyPreferences
import java.io.IOException
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import net.twinion.hummingbird.ui.panes.CollapseOverride
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobilePaneBand
import uniffi.hummingbird_ffi_mobile.MobileSurface

// `PanePrefs` is `FrontierPrefs`' sibling and inherits its rules verbatim —
// a failed read is the default (empty map), a failed write is silence — so
// these are that file's tests re-aimed, through the same `internal`
// store-taking overloads (the four public doors reach the store through a
// `Context`, which a plain JVM test cannot construct; this module runs no
// Robolectric).
//
// Two things here are `PanePrefs`' own rather than inherited, and are the
// reason this file exists at all: the **per-surface namespacing** the
// header argues for (Now and Status store under separate keys, because the
// same question could one day rank on both surfaces and a collapse on one
// is not a preference about the other), and the empty-map case, which
// REMOVES the key rather than storing an empty string.
//
// The collapse map's own semantics — band scoping, resurrection, the
// string form's skip-a-bad-line degradation — are `PaneCollapseTest`'s,
// where they are pure. This file is only the round trip and the
// degradation rule.
class PanePrefsTest {

    /** A store that fails the way DataStore really fails — [IOException] on
     * a corrupt file, a full disk, or a read before first unlock. */
    private class FailingStore(private val failure: Throwable) : DataStore<Preferences> {
        override val data: Flow<Preferences> = flow { throw failure }

        override suspend fun updateData(
            transform: suspend (Preferences) -> Preferences,
        ): Preferences = throw failure
    }

    /** A store that works, so a failure below is the store's and not the
     * fake's. */
    private class WorkingStore : DataStore<Preferences> {
        var stored: Preferences = emptyPreferences()

        override val data: Flow<Preferences> = flow { emit(stored) }

        override suspend fun updateData(
            transform: suspend (Preferences) -> Preferences,
        ): Preferences = transform(stored).also { stored = it }
    }

    private val dormantCollapsed = CollapseOverride(MobilePaneBand.DORMANT, collapsed = true)
    private val liveOpen = CollapseOverride(MobilePaneBand.LIVE, collapsed = false)

    @Test
    fun `a working store round-trips a collapse map through both doors`() = runBlocking {
        val store = WorkingStore()
        val map = mapOf(
            "waste:default" to dormantCollapsed,
            "race:f1" to liveOpen,
        )

        PanePrefs.writeCollapse(store, MobileSurface.NOW, map)

        assertEquals(map, PanePrefs.readCollapse(store, MobileSurface.NOW))
    }

    @Test
    fun `Now and Status never read each other's collapses`() = runBlocking {
        val store = WorkingStore()
        val nowMap = mapOf("waste:default" to dormantCollapsed)
        val statusMap = mapOf("uptime:hb.twinion.net" to liveOpen)

        PanePrefs.writeCollapse(store, MobileSurface.NOW, nowMap)
        PanePrefs.writeCollapse(store, MobileSurface.STATUS, statusMap)

        // The namespacing the header argues for: one store, two keys, and
        // writing either surface leaves the other's entry untouched.
        assertEquals(nowMap, PanePrefs.readCollapse(store, MobileSurface.NOW))
        assertEquals(statusMap, PanePrefs.readCollapse(store, MobileSurface.STATUS))
        assertNotEquals(
            "a shared key would make these two the same stored map",
            PanePrefs.readCollapse(store, MobileSurface.NOW),
            PanePrefs.readCollapse(store, MobileSurface.STATUS),
        )
    }

    @Test
    fun `emptying a surface's map clears what was stored for it, and only it`() = runBlocking {
        val store = WorkingStore()
        val statusMap = mapOf("uptime:hb.twinion.net" to liveOpen)
        PanePrefs.writeCollapse(store, MobileSurface.NOW, mapOf("waste:default" to dormantCollapsed))
        PanePrefs.writeCollapse(store, MobileSurface.STATUS, statusMap)

        // Every override on a surface can be taken back — `PaneCollapse.write`
        // prunes unranked keys, so a surface whose panes all left ranking
        // arrives here as an empty map. It removes the key rather than
        // storing an empty string, and reads back as nothing collapsed.
        PanePrefs.writeCollapse(store, MobileSurface.NOW, emptyMap())

        assertEquals(emptyMap<String, CollapseOverride>(), PanePrefs.readCollapse(store, MobileSurface.NOW))
        assertEquals(statusMap, PanePrefs.readCollapse(store, MobileSurface.STATUS))
    }

    @Test
    fun `an unread surface reads as nothing collapsed`() = runBlocking {
        val store = WorkingStore()

        assertEquals(
            "an untouched store must degrade to the shell's own default rule, not an error",
            emptyMap<String, CollapseOverride>(),
            PanePrefs.readCollapse(store, MobileSurface.STATUS),
        )
    }

    @Test
    fun `an unreadable store reads as nothing collapsed`() = runBlocking {
        val store = FailingStore(IOException("corrupt"))

        for (surface in MobileSurface.entries) {
            assertEquals(
                "a read that cannot reach the store must degrade to an empty map",
                emptyMap<String, CollapseOverride>(),
                PanePrefs.readCollapse(store, surface),
            )
        }
    }

    @Test
    fun `an unwritable store swallows the write rather than failing the pane`() = runBlocking {
        val store = FailingStore(IOException("no space left on device"))

        // No assertion beyond "returns" — silence IS the behaviour. An
        // unhandled throw here would abort a tap on a pane header, which is
        // a view preference and never worth an error.
        PanePrefs.writeCollapse(
            store,
            MobileSurface.NOW,
            mapOf("waste:default" to dormantCollapsed),
        )
    }

    @Test
    fun `a cancelled read is not absorbed the way a store failure is`() {
        // `FrontierPrefsTest`'s own decision-pinning test, and for the same
        // reason: `tolerating` catches [IOException] *specifically* rather
        // than blanket-`runCatching`, because a paused screen's cancelled
        // reload job throws [CancellationException] through these same calls
        // and a coroutine must never absorb it. A `runCatching` refactor
        // would keep every other test in this file green.
        val store = FailingStore(CancellationException("the screen paused"))

        for (call in listOf<suspend () -> Any?>(
            { PanePrefs.readCollapse(store, MobileSurface.NOW) },
            { PanePrefs.writeCollapse(store, MobileSurface.NOW, mapOf("waste:default" to dormantCollapsed)) },
        )) {
            try {
                runBlocking { call() }
                throw AssertionError(
                    "cancellation must propagate — only IOException degrades to a default",
                )
            } catch (_: CancellationException) {
                // The one exception these doors must never swallow.
            }
        }
    }

    // ---------------------------------------------- the open chip (#689)

    @Test
    fun `a working store round-trips the open chip through both doors`() = runBlocking {
        val store = WorkingStore()

        PanePrefs.writeExpanded(store, MobileSurface.STATUS, "uptime:runner")

        assertEquals("uptime:runner", PanePrefs.readExpanded(store, MobileSurface.STATUS))
    }

    @Test
    fun `nothing open is the key's absence, not a stored blank`() = runBlocking {
        val store = WorkingStore()
        PanePrefs.writeExpanded(store, MobileSurface.STATUS, "uptime:runner")

        PanePrefs.writeExpanded(store, MobileSurface.STATUS, null)

        assertNull(PanePrefs.readExpanded(store, MobileSurface.STATUS))
        assertEquals(emptyPreferences(), store.data.first())
    }

    @Test
    fun `an unread surface has nothing open`() = runBlocking {
        assertNull(PanePrefs.readExpanded(WorkingStore(), MobileSurface.STATUS))
    }

    /** The open chip is a different preference from the collapse map, and a
     * key of its own — writing one must not disturb the other. */
    @Test
    fun `the open chip is namespaced away from the collapse map`() = runBlocking {
        val store = WorkingStore()
        val collapses = mapOf("uptime:runner" to dormantCollapsed)

        PanePrefs.writeCollapse(store, MobileSurface.STATUS, collapses)
        PanePrefs.writeExpanded(store, MobileSurface.STATUS, "kimi:balance")

        assertEquals(collapses, PanePrefs.readCollapse(store, MobileSurface.STATUS))
        assertEquals("kimi:balance", PanePrefs.readExpanded(store, MobileSurface.STATUS))
    }

    @Test
    fun `Now and Status never read each other's open chip`() = runBlocking {
        val store = WorkingStore()

        PanePrefs.writeExpanded(store, MobileSurface.NOW, "waste:default")
        PanePrefs.writeExpanded(store, MobileSurface.STATUS, "uptime:runner")

        assertEquals("waste:default", PanePrefs.readExpanded(store, MobileSurface.NOW))
        assertEquals("uptime:runner", PanePrefs.readExpanded(store, MobileSurface.STATUS))
    }

    @Test
    fun `an unreadable store has nothing open, and an unwritable one stays silent`() = runBlocking {
        val store = FailingStore(IOException("disk"))

        assertNull(PanePrefs.readExpanded(store, MobileSurface.STATUS))
        PanePrefs.writeExpanded(store, MobileSurface.STATUS, "uptime:runner")
    }
}
