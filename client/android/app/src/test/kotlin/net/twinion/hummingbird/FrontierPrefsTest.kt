package net.twinion.hummingbird

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.emptyPreferences
import java.io.IOException
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis

// `FrontierPrefs`'s own header: "a store that cannot be read or written is
// not an error the board is allowed to fail on" — a failed read is the
// default, a failed write is silence. That rule is the whole reason the
// file catches anything, and until now nothing exercised it: the four
// public doors reach the store through a `Context`, which a plain JVM test
// cannot construct (this module runs no Robolectric), so these go through
// the `internal` overloads that take the store directly.
//
// The last test here is the one that pins a decision rather than a
// behaviour: the header argues for catching [IOException] *specifically*
// over a blanket `runCatching`, because a paused screen's cancelled resume
// job throws [CancellationException] through these same calls and a
// coroutine must never absorb it. A `runCatching` refactor would keep every
// other test in this file green.
class FrontierPrefsTest {

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

    @Test
    fun `an unreadable store reads as the default axis, not as a failure`() = runBlocking {
        val store = FailingStore(IOException("corrupt"))

        assertEquals(
            "a read that cannot reach the store must degrade to the default axis",
            MobileFrontierAxis.CONTEXT,
            FrontierPrefs.readAxis(store),
        )
    }

    @Test
    fun `an unreadable store reads as an empty collapse set`() = runBlocking {
        val store = FailingStore(IOException("corrupt"))

        assertEquals(
            "a read that cannot reach the store must degrade to nothing collapsed",
            emptySet<String>(),
            FrontierPrefs.readCollapsedColumns(store),
        )
    }

    @Test
    fun `an unwritable store swallows both writes rather than failing the board`() = runBlocking {
        val store = FailingStore(IOException("no space left on device"))

        // No assertion beyond "returns" — silence IS the behaviour. An
        // unhandled throw here would abort an axis change before the
        // refresh that follows it in `NowViewModel.setAxis`.
        FrontierPrefs.writeAxis(store, MobileFrontierAxis.PROJECT)
        FrontierPrefs.writeCollapsedColumns(store, setOf("errands"))
    }

    @Test
    fun `a cancelled read is not absorbed the way a store failure is`() {
        val store = FailingStore(CancellationException("the screen paused"))

        for (read in listOf<suspend () -> Any>(
            { FrontierPrefs.readAxis(store) },
            { FrontierPrefs.readCollapsedColumns(store) },
            { FrontierPrefs.writeAxis(store, MobileFrontierAxis.SIZE) },
            { FrontierPrefs.writeCollapsedColumns(store, setOf("errands")) },
        )) {
            try {
                runBlocking { read() }
                throw AssertionError(
                    "cancellation must propagate — only IOException degrades to a default",
                )
            } catch (_: CancellationException) {
                // The one exception these doors must never swallow.
            }
        }
    }

    @Test
    fun `a working store round-trips through the same doors`() = runBlocking {
        val store = WorkingStore()

        FrontierPrefs.writeAxis(store, MobileFrontierAxis.ENERGY)
        FrontierPrefs.writeCollapsedColumns(store, setOf("errands", "calls"))

        assertEquals(MobileFrontierAxis.ENERGY, FrontierPrefs.readAxis(store))
        assertEquals(setOf("errands", "calls"), FrontierPrefs.readCollapsedColumns(store))
    }
}
