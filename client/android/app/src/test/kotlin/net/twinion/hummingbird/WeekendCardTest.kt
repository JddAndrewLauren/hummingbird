package net.twinion.hummingbird

import java.io.File
import java.time.ZoneId
import net.twinion.hummingbird.ui.panes.offersPlanChips
import net.twinion.hummingbird.ui.panes.plannedDayOf
import net.twinion.hummingbird.ui.panes.shortDayLabel
import net.twinion.hummingbird.ui.panes.weekendEntryTimeLabel
import net.twinion.hummingbird.ui.panes.weekendGapReason
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileWeekendEntry
import uniffi.hummingbird_ffi_mobile.MobileWeekendEntryAnchor
import uniffi.hummingbird_ffi_mobile.MobileWeekendEntryKind
import uniffi.hummingbird_ffi_mobile.MobileWeekendGap

// The weekend card's own pure halves (#621) — the rendering-side answers
// `NowPanesExpanded.kt` composes from the entries `weekend.rs` merged.
//
// **The merge itself is NOT tested here**, and could not be: the dedupe
// rule, the multi-day expansion and the ordering are `weekend.rs`'s, tested
// in Rust once for every client. What is testable here is the part that is
// genuinely this file's — which chip is filled, what an entry's time words
// are, and whether a chip is offered at all.
class WeekendCardTest {

    private fun entry(
        kind: MobileWeekendEntryKind,
        anchor: MobileWeekendEntryAnchor = MobileWeekendEntryAnchor.DAY,
        dayKey: String = "2026-08-15",
        alsoScheduledOn: String? = null,
        atMs: Long = 0L,
    ) = MobileWeekendEntry(
        id = "e",
        kind = kind,
        title = "t",
        atMs = atMs,
        anchor = anchor,
        dayKey = dayKey,
        sourceId = "item-1",
        alsoScheduledOn = alsoScheduledOn,
        deadlineOutsideWindow = null,
    )

    @Test
    fun `a scheduled entry is planned for its own day`() {
        assertEquals(
            "2026-08-15",
            plannedDayOf(entry(MobileWeekendEntryKind.SCHEDULED, dayKey = "2026-08-15")),
        )
    }

    @Test
    fun `a due entry is planned for the do-date the merge deduped away`() {
        // The dedupe kept one entry, as due; `alsoScheduledOn` is what it
        // suppressed, and it is the only way this card can know which chip
        // to fill.
        assertEquals(
            "2026-08-16",
            plannedDayOf(
                entry(
                    MobileWeekendEntryKind.DUE,
                    dayKey = "2026-08-15",
                    alsoScheduledOn = "2026-08-16",
                ),
            ),
        )
    }

    @Test
    fun `a due entry with no do-date is planned for nothing`() {
        assertNull(plannedDayOf(entry(MobileWeekendEntryKind.DUE)))
    }

    @Test
    fun `an event is never planned and never offers a chip`() {
        // A calendar event is the calendar's. This app writes no calendar
        // (ADR-0002 rule 1), so a chip on one would be a control with
        // nothing behind it.
        val event = entry(MobileWeekendEntryKind.EVENT)
        assertNull(plannedDayOf(event))
        assertFalse(offersPlanChips(event))
        assertTrue(offersPlanChips(entry(MobileWeekendEntryKind.DUE)))
        assertTrue(offersPlanChips(entry(MobileWeekendEntryKind.SCHEDULED)))
    }

    @Test
    fun `the time words read the entry's own anchor, never the timestamp`() {
        val zone = ZoneId.of("UTC")
        assertEquals(
            "anytime",
            weekendEntryTimeLabel(entry(MobileWeekendEntryKind.SCHEDULED), zone),
        )
        assertEquals(
            "by end of day",
            weekendEntryTimeLabel(
                entry(MobileWeekendEntryKind.DUE, MobileWeekendEntryAnchor.DAY),
                zone,
            ),
        )
        assertEquals(
            "all day",
            weekendEntryTimeLabel(
                entry(MobileWeekendEntryKind.EVENT, MobileWeekendEntryAnchor.DAY),
                zone,
            ),
        )
        // A timed entry AT local midnight still reads as timed — the
        // anchor says so, and an `atMs % DAY_MS == 0` test would call it
        // all-day.
        assertNotEquals(
            "all day",
            weekendEntryTimeLabel(
                entry(MobileWeekendEntryKind.EVENT, MobileWeekendEntryAnchor.TIME, atMs = 0L),
                zone,
            ),
        )
    }

    @Test
    fun `a day key reads as its own short weekday`() {
        // 2026-08-14 is a Friday.
        assertEquals("Fri", shortDayLabel("2026-08-14"))
        // Anything unparseable reads back as itself rather than throwing
        // inside a composition.
        assertEquals("not-a-date", shortDayLabel("not-a-date"))
    }

    @Test
    fun `every gap kind has its own sentence`() {
        val sentences = MobileWeekendGap.entries.map(::weekendGapReason)
        assertEquals(
            "no two gap kinds may share a sentence",
            sentences.size,
            sentences.toSet().size,
        )
    }

    @Test
    fun `the plan chip is the do-date write's caller`() {
        // `MobileTaskHost.setScheduledDate` and
        // `NowViewModel.setScheduledDate` shipped for this pane and sat
        // caller-less until #621. A source gate rather than a behavioural
        // one: this module runs no Robolectric, so a private `@Composable`
        // in `NowPanesExpanded.kt` cannot be composed in a JVM test.
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        fun source(relative: String) =
            File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/$relative")
                .readText()
                .replace(Regex("""/\*[\s\S]*?\*/"""), "")
                .replace(Regex("""(?m)^\s*//.*$"""), "")

        assertTrue(
            "the weekend card must write through onSetScheduledDate",
            source("ui/panes/NowPanesExpanded.kt").contains("onSetScheduledDate(entry.sourceId"),
        )
        assertTrue(
            "NowScreen must hand the pane section the view model's own write",
            source("NowScreen.kt").contains("viewModel.setScheduledDate("),
        )
    }
}
