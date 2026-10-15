import {
  fantasyAnswerFromCore,
  fantasyFactsFromCore,
  fantasyLeaguesFromBindingFromCore,
  fantasyParseSubjectKeyFromCore,
  fantasySetupFromCore,
  fantasySubjectsFromCore,
  type FantasyGap,
  type FantasySetupCore,
  type FantasySubjectCore,
  type PaneInputsSource,
} from "../../decisions/seam";
import type { PaneAnswer, QuestionInputs } from "../questions/contract";

// **The fantasy-football question** (#717) — the web's rendering half only.
//
// Every rule is `hummingbird_core::decisions::panes::fantasy`: the league
// list, the two subjects per league, the setup arm and the gap kinds. This
// slice answers only the two states a real device is in on day one —
// unbound, and bound but never polled — so what is here is the words for
// those, and nothing that reads a lineup or a waiver deadline yet.

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

/** Why a bound pane has no answer, in words. */
export function fantasyGapReason(subjectKey: string, inputs: QuestionInputs): string {
  const resolved = fantasyFactsFromCore(subjectKey, paneInputs(inputs));
  return gapReason(resolved.gap, subjectOf(subjectKey));
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
    default:
      return "Nothing to show yet.";
  }
}

const ICON = { lineup: "list-checks", waivers: "clock" } as const;

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
  const league = parsed === null ? subjectKey : leagueLabel(parsed.league);
  return {
    ...answer,
    collapsedHeadline: `${league} · ${label} · Never polled`,
    icon: [{ kind: "icon", name: ICON[subject], label: "never polled" }],
  };
}
