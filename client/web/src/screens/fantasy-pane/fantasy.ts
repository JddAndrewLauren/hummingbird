import {
  fantasyAnswerFromCore,
  fantasyDiscoveredLeaguesFromCore,
  fantasyFactsFromCore,
  fantasyLeaguesFromBindingFromCore,
  fantasyParseSubjectKeyFromCore,
  fantasySetupFromCore,
  fantasySubjectsFromCore,
  relativeAgeFromCore,
  type FantasyDiscoveredCore,
  type FantasyGap,
  type FantasySetupCore,
  type FantasySubjectCore,
  type LineupFactsCore,
  type LineupHeadlineCore,
  type PaneInputsSource,
  type SlotProblemCore,
} from "../../decisions/seam";
import type { PaneReadDTO } from "../../store/protocol";
import type { PaneAnswer, PaneGlyph, QuestionInputs } from "../questions/contract";

// **The fantasy-football question** (#717) — the web's rendering half only.
//
// Every rule is `hummingbird_core::decisions::panes::fantasy`: the league
// list, the two subjects per league, the setup arm, the gap kinds and —
// since #855 — the lineup's facts, band and headline. The headline crosses
// as a decision (which arm, the count, the shared reason, the lock instant,
// the crunch-time age); what is here is the words for it, and the
// device-local words for an instant ("Sun 1:00pm", "4h"), which ADR-0015
// leaves to each device. The waivers subject still answers only its no-data
// arms (#719).

export const LINEUP_SOURCE = "yahoo-lineup/v1";
export const WAIVERS_SOURCE = "yahoo-waivers/v1";
export const BINDING_KEY = "yahoo-leagues";
export const SETUP_LEAGUE = "setup";

export type FantasySubject = FantasySubjectCore;
export type FantasySetup = FantasySetupCore;

function paneInputs(inputs: QuestionInputs): PaneInputsSource {
  return { nowMs: inputs.nowMs, bindings: inputs.bindings, paneReads: inputs.paneReads };
}

/** `fantasy.rs`'s `leagues_from_binding`. */
export function leaguesFromBinding(text: string): string[] {
  return fantasyLeaguesFromBindingFromCore(text);
}

/** `fantasy.rs`'s `fantasy_setup`. */
export function fantasySetup(inputs: QuestionInputs): FantasySetup {
  return fantasySetupFromCore(paneInputs(inputs));
}

/** This question's subjects — `fantasy.rs`'s `fantasy_subjects`. */
export function fantasySubjects(inputs: QuestionInputs): string[] {
  return fantasySubjectsFromCore(paneInputs(inputs));
}

/** Which league and which obligation a subject key names, or `null`. */
export function parseSubjectKey(subjectKey: string): { league: string; subject: FantasySubject } | null {
  return fantasyParseSubjectKeyFromCore(subjectKey);
}

/** "Lineup" / "Waivers" — the obligation's name on screen. */
export function subjectLabel(subject: FantasySubject): string {
  return subject === "lineup" ? "Lineup" : "Waivers";
}

function subjectOf(subjectKey: string): FantasySubject {
  return parseSubjectKey(subjectKey)?.subject ?? "lineup";
}

/** The league as a reader names it: the key's own id after `.l.`, since a
 * league's display name arrives only with the poller (#718). */
export function leagueLabel(league: string): string {
  const id = league.split(".l.")[1];
  return id === undefined || id === "" ? league : `League ${id}`;
}

/** Why a bound pane has no answer, in words — read only when
 * [`lineupFacts`] returned `null`. */
export function fantasyGapReason(subjectKey: string, inputs: QuestionInputs): string {
  const resolved = fantasyFactsFromCore(subjectKey, paneInputs(inputs));
  return resolved.kind === "gap" ? gapReason(resolved.gap, subjectOf(subjectKey)) : "Nothing to show yet.";
}

/** The lineup's answered facts, or `null` when there is nothing to answer
 * with (no row, or one this build cannot read). */
export function lineupFacts(subjectKey: string, inputs: QuestionInputs): LineupFactsCore | null {
  const resolved = fantasyFactsFromCore(subjectKey, paneInputs(inputs));
  return resolved.kind === "lineup" ? resolved : null;
}

function gapReason(gap: FantasyGap, subject: FantasySubject): string {
  switch (gap.gap) {
    case "notFetched":
      return subject === "lineup"
        ? "This league's lineup has not been polled yet."
        : "This league's waivers have not been polled yet.";
    case "malformed":
      return `The payload couldn't be read: ${gap.reason}`;
    case "unknownSchema":
      return `This device doesn't know how to read ${gap.schema} yet. Update the app.`;
    case "bodyNotRead":
      return "A payload has landed that this build does not read yet. Update the app.";
    case "badBody":
      return "The lineup payload isn't one this app can read.";
    default:
      return "Nothing to show yet.";
  }
}

const ICON = { lineup: "list-checks", waivers: "clock" } as const;

// -- the lineup's words (#855) ------------------------------------------------

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

const PROBLEM_WORDS: Record<SlotProblemCore, [string, string]> = {
  empty: ["empty slot", "empty slots"],
  bye: ["starter on bye", "starters on bye"],
  out: ["starter out", "starters out"],
  injuredReserve: ["starter on IR", "starters on IR"],
};

/** "2 empty slots", "1 starter on bye" — or, when the invalid slots differ
 * in why, "3 slots to fix". Counts are bare (the brand's rule). */
export function problemWords(count: number, reason: SlotProblemCore | null): string {
  if (reason === null) {
    return `${count} ${count === 1 ? "slot" : "slots"} to fix`;
  }
  const [one, many] = PROBLEM_WORDS[reason];
  return `${count} ${count === 1 ? one : many}`;
}

/** Why one slot is invalid, for its expanded row. */
export function slotProblemWords(reason: SlotProblemCore): string {
  switch (reason) {
    case "empty":
      return "Empty";
    case "bye":
      return "On bye";
    case "out":
      return "Out";
    case "injuredReserve":
      return "On IR";
  }
}

/** The poller's injury designation, in words; an unrecognised one is shown
 * as it came rather than hidden. */
export function designationWords(designation: string): string {
  switch (designation) {
    case "questionable":
      return "Questionable";
    case "doubtful":
      return "Doubtful";
    case "out":
      return "Out";
    case "injured_reserve":
      return "IR";
    default:
      return designation;
  }
}

/** "4h", "45m", "3d" — a duration ahead, short (the brand's "Numbers and
 * time"), floored so a deadline never reads later than it is. */
export function shortDuration(deltaMs: number): string {
  const minutes = Math.floor(deltaMs / MINUTE_MS);
  if (minutes < 60) {
    return `${Math.max(minutes, 1)}m`;
  }
  const hours = Math.floor(deltaMs / HOUR_MS);
  if (hours < 48) {
    return `${hours}h`;
  }
  return `${Math.floor(deltaMs / DAY_MS)}d`;
}

/** "Sun 1:00pm" — this device's own day and wall clock, no zone suffix
 * (ADR-0015). A lock more than six days out, or already past, names its
 * date instead of a weekday that would read as this week's. */
export function lockTime(atMs: number, nowMs: number): string {
  const at = new Date(atMs);
  const day =
    atMs >= nowMs && atMs - nowMs < 6 * DAY_MS
      ? new Intl.DateTimeFormat("en-US", { weekday: "short" }).format(at)
      : new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric" }).format(at);
  const clock = new Intl.DateTimeFormat("en-US", { hour: "numeric", minute: "2-digit" })
    .format(at)
    .replace(/\s?([AP])M$/i, (_, half: string) => `${half.toLowerCase()}m`);
  return `${day} ${clock}`;
}

/** The core's headline, in words: `Lineup set — locks Sun 1:00pm`, or
 * `2 empty slots — locks in 4h`; a crunch-time answer hours old says so
 * (`2 empty slots, last checked 3h ago — locks in 4h`). */
export function lineupHeadline(headline: LineupHeadlineCore, nowMs: number): string {
  if (headline.kind === "set") {
    return headline.locksAtMs === null
      ? "Lineup set"
      : `Lineup set — locks ${lockTime(headline.locksAtMs, nowMs)}`;
  }
  const problem = problemWords(headline.count, headline.reason);
  const checked =
    headline.checkedAgoMs === null ? "" : `, last checked ${relativeAgeFromCore(headline.checkedAgoMs)}`;
  if (headline.locksAtMs === null) {
    return `${problem}${checked}`;
  }
  const lock =
    headline.locksAtMs <= nowMs ? "lock passed" : `locks in ${shortDuration(headline.locksAtMs - nowMs)}`;
  return `${problem}${checked} — ${lock}`;
}

function lineupGlyphs(headline: LineupHeadlineCore): PaneGlyph[] {
  if (headline.kind === "set") {
    return [{ kind: "icon", name: "circle-check", label: "lineup set" }];
  }
  const glyphs: PaneGlyph[] = [{ kind: "icon", name: "list-checks", label: "lineup needs fixing" }];
  if (headline.checkedAgoMs !== null) {
    glyphs.push({ kind: "icon", name: "cloud-fog", label: "not checked recently" });
  }
  return glyphs;
}

// -- the discovered leagues (#855) -------------------------------------------

/** The poller's discovered leagues for Settings — `fantasy.rs`'s
 * `fantasy_discovered_leagues`, over the same pane reads Now uses. Neither
 * the clock nor the bindings bear on it. */
export function discoveredLeagues(paneReads: Record<string, PaneReadDTO | undefined>): FantasyDiscoveredCore {
  return fantasyDiscoveredLeaguesFromCore({ nowMs: 0, bindings: null, paneReads });
}

/** This question's answer for the shell (#245/#717) — the core decides the
 * state and band; this adds the collapsed row's words and glyph. */
export function fantasyAnswer(subjectKey: string, inputs: QuestionInputs): PaneAnswer {
  const answer = fantasyAnswerFromCore(subjectKey, paneInputs(inputs));
  const parsed = parseSubjectKey(subjectKey);
  const subject = parsed?.subject ?? "lineup";
  const label = subjectLabel(subject);
  const setup = fantasySetup(inputs);

  if (setup.kind === "unset") {
    return {
      ...answer,
      collapsedHeadline: `${label} · Not set up`,
      icon: [{ kind: "icon", name: "help-circle", label: "not set up" }],
    };
  }
  if (setup.kind !== "bound") {
    return {
      ...answer,
      collapsedHeadline: `${label} · ${setup.kind === "unread" ? "Checking setup" : "Setup needs a look"}`,
      icon: [
        setup.kind === "unread"
          ? { kind: "icon", name: "cloud-fog", label: "checking setup" }
          : { kind: "icon", name: "help-circle", label: "setup needs a look" },
      ],
    };
  }
  const facts = subject === "lineup" ? lineupFacts(subjectKey, inputs) : null;
  if (facts !== null) {
    return {
      ...answer,
      collapsedHeadline: `${facts.leagueName} · ${lineupHeadline(facts.headline, inputs.nowMs)}`,
      icon: lineupGlyphs(facts.headline),
    };
  }
  const league = parsed === null ? subjectKey : leagueLabel(parsed.league);
  return {
    ...answer,
    collapsedHeadline: `${league} · ${label} · Never polled`,
    icon: [{ kind: "icon", name: ICON[subject], label: "never polled" }],
  };
}
