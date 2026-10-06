package net.twinion.hummingbird

import net.twinion.hummingbird.ui.panes.paneGlyphs
import net.twinion.hummingbird.ui.panes.paneHeadline
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneBand
import uniffi.hummingbird_ffi_mobile.MobilePaneFacts
import uniffi.hummingbird_ffi_mobile.MobileFantasyGap
import uniffi.hummingbird_ffi_mobile.MobileFantasyResolved
import uniffi.hummingbird_ffi_mobile.MobileFantasySetup
import uniffi.hummingbird_ffi_mobile.MobileFantasySubject
import uniffi.hummingbird_ffi_mobile.MobileRaceGap
import uniffi.hummingbird_ffi_mobile.MobileRaceResolved
import uniffi.hummingbird_ffi_mobile.MobileRaceSetup
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.MobileWasteGap
import uniffi.hummingbird_ffi_mobile.MobileWasteResolved
import uniffi.hummingbird_ffi_mobile.MobileWasteSetup

// The waste and race panes fold three different situations onto one
// `bound-but-unacquired` answer state, so the setup kind the seam carries
// beside the facts is the only thing that can separate them. These are the
// words `waste.ts` and `race.ts` already draw for the same three arms — a
// port test, not a new decision.
//
// Why it is worth pinning: the failure is silent. Before the setup kind
// crossed, an unusable binding rendered as "No answer yet" / "· Never
// polled" — indistinguishable from a poller that simply has not run, so a
// reader had no reason to go and repair it in Settings.
class PaneSetupWordsTest {

    private fun pane(question: MobileStandingQuestion, facts: MobilePaneFacts) = MobileRankedPane(
        standingQuestion = question,
        subjectKey = "f1",
        paneKey = "${question.name.lowercase()}:f1",
        answer = MobilePaneAnswer(
            answerState = MobilePaneAnswerState.BOUND_BUT_UNACQUIRED,
            band = MobilePaneBand.DORMANT,
            withinBand = null,
        ),
        facts = facts,
    )

    private fun wastePane(setup: MobileWasteSetup) = pane(
        MobileStandingQuestion.WASTE,
        MobilePaneFacts.Waste(
            setup = setup,
            resolved = MobileWasteResolved.Gap(gap = MobileWasteGap.NotFetched),
        ),
    )

    private fun racePane(setup: MobileRaceSetup) = pane(
        MobileStandingQuestion.RACE,
        MobilePaneFacts.Race(
            setup = setup,
            resolved = MobileRaceResolved.Gap(gap = MobileRaceGap.NotFetched),
        ),
    )

    @Test
    fun `the waste pane's unacquired words follow its setup kind`() {
        assertEquals("Checking setup", paneHeadline(wastePane(MobileWasteSetup.UNREAD), 0L))
        assertEquals("Setup needs a look", paneHeadline(wastePane(MobileWasteSetup.UNUSABLE), 0L))
        assertEquals("No answer yet", paneHeadline(wastePane(MobileWasteSetup.BOUND), 0L))
    }

    @Test
    fun `the race pane's unacquired words follow its setup kind`() {
        assertEquals("Checking setup", paneHeadline(racePane(MobileRaceSetup.UNREAD), 0L))
        assertEquals("Setup needs a look", paneHeadline(racePane(MobileRaceSetup.UNUSABLE), 0L))
        // A binding that IS bound and simply has no poll yet keeps the
        // series label: this arm is about the series, the two above are
        // about the setup, which names no series because none is
        // established.
        assertEquals("F1 · Never polled", paneHeadline(racePane(MobileRaceSetup.BOUND), 0L))
    }

    private fun fantasyPane(setup: MobileFantasySetup, subject: MobileFantasySubject, key: String) =
        pane(
            MobileStandingQuestion.FANTASY,
            MobilePaneFacts.Fantasy(
                setup = setup,
                subject = subject,
                resolved = MobileFantasyResolved.Gap(gap = MobileFantasyGap.NotFetched),
            ),
        ).copy(subjectKey = key)

    @Test
    fun `the fantasy panes' no-data words are fantasy_ts's`() {
        assertEquals(
            "Lineup · Not set up",
            paneHeadline(fantasyPane(MobileFantasySetup.UNSET, MobileFantasySubject.LINEUP, "setup:lineup"), 0L),
        )
        assertEquals(
            "Waivers · Checking setup",
            paneHeadline(fantasyPane(MobileFantasySetup.UNREAD, MobileFantasySubject.WAIVERS, "setup:waivers"), 0L),
        )
        assertEquals(
            "League 123456 · Waivers · Never polled",
            paneHeadline(
                fantasyPane(MobileFantasySetup.BOUND, MobileFantasySubject.WAIVERS, "449.l.123456:waivers"),
                0L,
            ),
        )
    }

    @Test
    fun `the setup marks are named, and the two kinds do not share one`() {
        val unread = paneGlyphs(wastePane(MobileWasteSetup.UNREAD), 0L).single()
        val unusable = paneGlyphs(wastePane(MobileWasteSetup.UNUSABLE), 0L).single()
        assertEquals("checking setup", unread.label)
        assertEquals("setup needs a look", unusable.label)
        assertEquals(unread.label, paneGlyphs(racePane(MobileRaceSetup.UNREAD), 0L).single().label)
        assertEquals(
            unusable.label,
            paneGlyphs(racePane(MobileRaceSetup.UNUSABLE), 0L).single().label,
        )
    }
}
