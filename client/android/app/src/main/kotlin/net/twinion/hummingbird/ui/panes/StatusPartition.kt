package net.twinion.hummingbird.ui.panes

import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneBand

// Which half of the Status quiet stack a pane belongs to (ADR-0017
// decision 1, in the operator's own words: "all green is one quiet stack,
// red announces itself").
//
// **This decides nothing.** It reads two values the core already decided —
// `band` and `answerState` — exactly as [PaneCollapse.defaultCollapsed]
// does, and on the same footing: a rendering rule expressed as a function of
// a decision, not a second judgement about a pane. Nothing here parses a
// payload, bands anything, or reorders: the seam's own order survives a
// `partition {}` in both halves, which is why the screen splits rather than
// sorts.
object StatusPartition {

    /** A pane announces itself unless it is *both* answered and dormant.
     *
     * The two consequences worth naming, both intended:
     *
     * - **A gap announces.** A pane nobody has polled has no answer to call
     *   "as expected", so hiding it behind a chip would make the quiet
     *   card's "N as expected" a lie. It gets a card and says, in its own
     *   words, why it has nothing.
     * - **A pane that has not been answered announces**, which is what
     *   keeps its "Open Settings" door on screen. Note what this is *not*
     *   gated on: **no Status question can ever be `UNBOUND`.** None of the
     *   four has a per-device binding to be unbound from
     *   (`panes/mod.rs`'s own test), so every gap here is
     *   `BOUND_BUT_UNACQUIRED`, and a rule written against `UNBOUND` would
     *   be a rule that never fires. On a device with no credentials that
     *   means the whole screen is announcing cards and there is no quiet
     *   card at all — the honest reading, and the one
     *   `client/android/README.md`'s hardware check 2 describes.
     *
     * Exhaustive over [MobilePaneBand] with no `else ->` arm, this screen's
     * own drift rule: a sixth band is a compile error here rather than a
     * pane that silently goes quiet. */
    fun isProblem(answer: MobilePaneAnswer): Boolean {
        if (answer.answerState != MobilePaneAnswerState.ANSWERED) return true
        return when (answer.band) {
            MobilePaneBand.LIVE,
            MobilePaneBand.IMMINENT,
            MobilePaneBand.NEAR,
            MobilePaneBand.DISTANT,
            -> true
            MobilePaneBand.DORMANT -> false
        }
    }
}
