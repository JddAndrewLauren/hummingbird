import { placeholderQuestion } from "../questions/placeholder";

// "Can this device itself reach the authority?" (ADR-0017's "what this
// obliges", #316 — pure client work answering the surface split with no new
// source, no new credential and no schema change: the one pane only the
// device itself can answer). Replaced wholesale once #316 lands; see
// `placeholder.ts`'s header for why this is a call, not a hand-shaped
// `QuestionDef` of its own.
export const reachabilityQuestion = placeholderQuestion("This device", "reachability");
