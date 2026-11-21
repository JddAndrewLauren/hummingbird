package net.twinion.hummingbird.notify

import net.twinion.hummingbird.notify.ThreadGroups.Posted
import org.junit.Assert.assertEquals
import org.junit.Test

// "The summary goes away when its last alert is cancelled or acked" (#922) —
// the rule, without a notification manager.
class ThreadGroupsTest {

    private fun alert(id: String, group: String?) = Posted(tag = id, id = 1, group = group, isSummary = false)
    private fun summary(thread: String) =
        Posted(tag = ThreadGroups.summaryTag(thread), id = 2, group = thread, isSummary = true)

    @Test
    fun `a summary with no alert left under it is orphaned`() {
        assertEquals(listOf(summary("t")), ThreadGroups.orphanedSummaries(listOf(summary("t"))))
    }

    @Test
    fun `a summary with an alert still under it stays`() {
        assertEquals(
            emptyList<Posted>(),
            ThreadGroups.orphanedSummaries(listOf(summary("t"), alert("a-1", "t"))),
        )
    }

    @Test
    fun `cancelling one of two alerts keeps the summary and cancelling the last takes it`() {
        val both = listOf(summary("t"), alert("a-1", "t"), alert("a-2", "t"))
        assertEquals(emptyList<Posted>(), ThreadGroups.orphanedSummaries(both.filter { it.tag != "a-1" }))
        assertEquals(
            listOf(summary("t")),
            ThreadGroups.orphanedSummaries(both.filter { it.tag != "a-1" && it.tag != "a-2" }),
        )
    }

    @Test
    fun `only the emptied thread's summary goes`() {
        val active = listOf(summary("t1"), summary("t2"), alert("a-2", "t2"), alert("loose", null))
        assertEquals(listOf(summary("t1")), ThreadGroups.orphanedSummaries(active))
    }

    @Test
    fun `a summary this lane did not tag is never returned`() {
        val foreign = Posted(tag = "someone-else", id = 9, group = "g", isSummary = true)
        assertEquals(emptyList<Posted>(), ThreadGroups.orphanedSummaries(listOf(foreign)))
    }

    @Test
    fun `two overlapping cancels against a stale snapshot still retire the summary`() {
        // Both workers read a snapshot taken before either cancel landed: it
        // still lists both children.
        val stale = listOf(summary("t"), alert("a-1", "t"), alert("a-2", "t"))
        val book = SummaryBookkeeping()
        assertEquals(emptyList<Posted>(), book.cancelled("a-1", stale))
        assertEquals(listOf(summary("t")), book.cancelled("a-2", stale))
    }

    @Test
    fun `an alert posted again after its cancel counts as a child again`() {
        val stale = listOf(summary("t"), alert("a-1", "t"))
        val book = SummaryBookkeeping()
        assertEquals(listOf(summary("t")), book.cancelled("a-1", stale))
        book.posted("a-1")
        assertEquals(emptyList<Posted>(), book.sweep(stale))
    }

    @Test
    fun `an id is forgotten once a snapshot no longer lists it`() {
        val book = SummaryBookkeeping()
        book.cancelled("a-1", listOf(summary("t"), alert("a-1", "t")))
        book.sweep(listOf(summary("t")))
        // a-1 re-posted by something that bypassed `posted` would count again.
        assertEquals(emptyList<Posted>(), book.sweep(listOf(summary("t"), alert("a-1", "t"))))
    }
}
