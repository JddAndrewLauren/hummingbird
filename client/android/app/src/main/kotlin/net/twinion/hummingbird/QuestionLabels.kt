package net.twinion.hummingbird

import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileStandingQuestion
import uniffi.hummingbird_ffi_mobile.questionRoster

// Every standing question's name on the phone, read off the core's roster
// (#716, ADR-0034 decision 4) — the one source of it in this app.
//
// **No question's words are spelled in Kotlin.** Until #716 `NowScreen.kt`'s
// `nowPaneLabel` and `StatusScreen.kt`'s `paneLabel` each held a `when` of
// literals — the per-client table ADR-0034 decision 4 refuses: an eleventh
// question meant editing the core's `SUNK` *and* remembering two Kotlin
// files. Both now call [rosterPaneLabel], and Settings' roster section reads
// the same entries whole. `QuestionRosterLiteralTest` fails if a label, a
// binding key or a question order is reintroduced anywhere in the app's
// Kotlin.
//
// What stays here is rendering only: whether a pane's row names its subject
// after the question. That is a choice about a row with several siblings on
// one surface (four race series, one tile per workflow), not about what the
// question is called, and the web makes it differently (its tiles carry the
// subject in the headline) — ADR-0025's per-client line.

/** The roster's labels by question — a constant of the build, read once.
 * `lazy` so nothing touches the native library until a pane actually
 * renders. */
internal object QuestionRosterLabels {
    val labels: Map<MobileStandingQuestion, String> by lazy {
        questionRoster().associate { it.question to it.label }
    }
}

/** One ranked pane's label: the roster's name for its question, and for a
 * question that renders one pane per subject, that subject after it.
 *
 * A question missing from [labels] is unreachable (the roster covers every
 * sunk question — `decisions::questions`' own test) and says so rather than
 * putting an enum name on screen where a question's name belongs. */
internal fun rosterPaneLabel(
    pane: MobileRankedPane,
    labels: Map<MobileStandingQuestion, String>,
): String {
    val label = labels[pane.standingQuestion]
        ?: error("no roster entry for ${pane.standingQuestion}")
    return if (namesSubject(pane.standingQuestion)) "$label — ${pane.subjectKey}" else label
}

/** Whether a question's rows carry their subject — exhaustive with no
 * `else ->`, so a new question is a compile error here rather than a row
 * that silently guesses. */
private fun namesSubject(question: MobileStandingQuestion): Boolean = when (question) {
    MobileStandingQuestion.RACE,
    MobileStandingQuestion.FANTASY,
    MobileStandingQuestion.GITHUB,
    MobileStandingQuestion.UPTIME,
    MobileStandingQuestion.POLLER -> true
    MobileStandingQuestion.HOMEWORK,
    MobileStandingQuestion.SCPS,
    MobileStandingQuestion.WASTE,
    MobileStandingQuestion.WEEKEND,
    MobileStandingQuestion.VACATION,
    MobileStandingQuestion.KIMI,
    MobileStandingQuestion.REACHABILITY -> false
}
