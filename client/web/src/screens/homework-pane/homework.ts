import {
  homeworkAnswerFromCore,
  homeworkFactsFromCore,
  homeworkLinkFromCore,
  homeworkZoneQueriesFromCore,
  type HomeworkFactsCore,
  type PaneInputsSource,
} from "../../decisions/seam";
import { resolveZoneFacts } from "../questions/zone-bridge";
import type { PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";

// **The homework pane** (#675), answered over #245's pane shell — the web's
// rendering half only.
//
// Every rule is `hummingbird_core::decisions::panes::homework`: which items
// count as open, which one wins, the band table, and the civil-date
// arithmetic behind `daysAway` (which needs the zone bridge and so could
// not be redone here even if that were wanted). Read that module for the
// reasoning behind any of it, including the recorded objection to
// `@homework` being a Context at all.
//
// What stays here is what ADR-0025 leaves per-client: the sentence
// (`homeworkHeadline`) and the glyphs. **Gaps cross as kinds, never as
// sentences** — `daysAway` is a number, and the words below are this
// client's own.

/** The one subject this question ever has, mirroring
 * `homework.rs`'s `SUBJECT_KEY`. Named here rather than read through
 * `homeworkConstantsFromCore()` for `zone-bridge.ts`'s own reason: a seam
 * round trip per render buys nothing for a sentinel that cannot change at
 * runtime, and `homework.test.ts` pins the two together. */
export const SUBJECT_KEY = "homework";

/** The `settings` key the standing session link is held under, mirroring
 * `homework.rs`'s `HOMEWORK_LINK_BINDING_KEY` — named here for the same
 * reason `SUBJECT_KEY` is, and pinned against the core by
 * `homework.test.ts`. Only the demo fixture and the settings copy need the
 * spelling; every *reader* goes through `homeworkLink`. */
export const LINK_BINDING_KEY = "homework-link";

function paneInputs(inputs: QuestionInputs): PaneInputsSource {
  return {
    nowMs: inputs.nowMs,
    bindings: inputs.bindings,
    paneReads: inputs.paneReads,
    items: inputs.items,
  };
}

/** The whole answered fact set an expanded rendering draws, or `null` when
 * this device cannot resolve its own zone — `raceView`'s own shape.
 *
 * Runs the bridge's two phases in order: name the queries, resolve them
 * here, decide there. Every reader goes through this, so none of them can
 * answer against a half-resolved table. */
export function homeworkView(inputs: QuestionInputs): HomeworkFactsCore | null {
  const source = paneInputs(inputs);
  const resolved = homeworkFactsFromCore(source, resolveZoneFacts(homeworkZoneQueriesFromCore(source)));
  return resolved.kind === "facts" ? resolved : null;
}

/** The standing session link, or `null` when nothing usable is bound —
 * `homeworkView`'s thin wrapper, taking the same `QuestionInputs` every
 * other reader here does.
 *
 * Deliberately *not* derived from the facts: it is offered in every state
 * this pane has, including "No open homework" and the zone gap, because a
 * standing link is standing. It is also not attached to the winning item —
 * the pane is where it lives, not the homework. */
export function homeworkLink(inputs: QuestionInputs): string | null {
  return homeworkLinkFromCore(paneInputs(inputs));
}

/** This question's subjects — always exactly one, and never zero: nobody
 * binds this question, so a pane that vanished when nothing was open would
 * be a question nobody could discover was being asked. */
export function homeworkSubjects(): string[] {
  return [SUBJECT_KEY];
}

/** "3 days" · "1 day" — the count and its unit together, since this pane's
 * sentence sets them in one line rather than `race.ts`'s split. */
function days(count: number): string {
  return count === 1 ? "1 day" : `${count} days`;
}

/** The whole answer in one line. The forms are fixed (#675's own decision
 * table) and each says the deadline in the reader's terms rather than a
 * date they would have to subtract from today themselves. */
export function homeworkHeadline(facts: HomeworkFactsCore): string {
  if (facts.winner === null) {
    return "No open homework";
  }
  const away = facts.daysAway;
  if (away === null) {
    // There is homework; it simply carries no deadline. Saying "Homework"
    // and stopping is the honest version — a fabricated "someday" would be
    // this pane inventing a date nobody set.
    return "Homework";
  }
  if (away < 0) {
    return `Homework ${days(-away)} overdue`;
  }
  if (away === 0) {
    return "Homework due today";
  }
  if (away === 1) {
    return "Homework due tomorrow";
  }
  return `Homework due in ${days(away)}`;
}

function glyphs(facts: HomeworkFactsCore): PaneGlyph[] {
  if (facts.winner === null) {
    return [{ kind: "icon", name: "circle-check", label: "nothing open" }];
  }
  const marks: PaneGlyph[] =
    facts.daysAway !== null && facts.daysAway <= 0
      ? [{ kind: "icon", name: "flag", label: "due" }]
      : [{ kind: "icon", name: "scroll-text", label: "homework" }];
  if (facts.others.length > 0) {
    marks.push({
      kind: "icon",
      name: "list-checks",
      label: `${facts.others.length} more open`,
    });
  }
  return marks;
}

/** This question's answer for the shell (#245/#675). The three decided
 * fields come from `homework.rs`'s `homework_answer`; the headline and the
 * glyphs are composed here, exactly the cut ADR-0025 draws through
 * `PaneAnswer`. */
export function homeworkAnswer(_subjectKey: string, inputs: QuestionInputs): PaneAnswer {
  const source = paneInputs(inputs);
  const zone = resolveZoneFacts(homeworkZoneQueriesFromCore(source));
  const answer = homeworkAnswerFromCore(source, zone);
  const view = homeworkFactsFromCore(source, zone);
  if (view.kind !== "facts") {
    // The device could not say what day it is here. Never the setup
    // prompt: there is nothing to set up, so "go and configure this" would
    // be a wrong answer rather than a slow one.
    return {
      ...answer,
      collapsedHeadline: "Can't read this device's time zone",
      icon: [{ kind: "icon", name: "cloud-fog", label: "time zone unreadable" }],
    };
  }
  return { ...answer, collapsedHeadline: homeworkHeadline(view), icon: glyphs(view) };
}
