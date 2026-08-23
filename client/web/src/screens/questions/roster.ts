import { questionRosterFromCore, type QuestionRosterEntry } from "../../decisions/seam";
import type { StandingQuestion } from "./contract";

// The web's one door onto the core's standing-question roster (#714,
// ADR-0034 decision 4).
//
// **Why the label is not in this file.** Until this slice every question
// declared its own `label` on its `QuestionDef`, and the Settings screen
// hand-wrote a second copy of one of them into the calendar hint ("Polled
// because it answers *How long to the next vacation*"). That is the
// per-client table ADR-0034 decision 4 refuses: an eleventh question would
// mean editing the core's `SUNK` *and* remembering a file in another
// language. The wording is unchanged — the core took the web registry's own
// strings as canonical — but there is now exactly one copy of it, and
// Android reads the same list from its own seam.
//
// Memoised because the roster is a constant of the build, and `questionLabel`
// is called per pane per render on both ranked surfaces. The first call
// still goes through `required()` in `seam.ts`, so a consumer mounted before
// `initDecisions()` throws exactly as every other decision does — never a
// stale TS fallback.

let cached: QuestionRosterEntry[] | null = null;

/** Every standing question, in the core's `QUESTION_ORDER`. */
export function questionRoster(): QuestionRosterEntry[] {
  if (cached === null) {
    cached = questionRosterFromCore();
  }
  return cached;
}

function entry(question: StandingQuestion): QuestionRosterEntry {
  const found = questionRoster().find((candidate) => candidate.question === question);
  if (found === undefined) {
    // Unreachable through the type: `StandingQuestion` is the same closed
    // vocabulary the roster is built from. Stated rather than silently
    // returning the raw key, which would put a kebab-case word on screen
    // where a question's name belongs.
    throw new Error(`no roster entry for ${question}`);
  }
  return found;
}

/** One question's operator-facing name. The only source of it in this
 * client. */
export function questionLabel(question: StandingQuestion): string {
  return entry(question).label;
}
