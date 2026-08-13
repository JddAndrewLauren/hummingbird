import { placeholderQuestion } from "../questions/placeholder";

// "What's left of my Moonshot balance?" (ADR-0017 decision 5, #313's
// `kimi-balance/v1`). Replaced wholesale once that poller exists — see
// `placeholder.ts`'s header for why this is a call, not a hand-shaped
// `QuestionDef` of its own.
export const kimiQuestion = placeholderQuestion("Kimi balance", "kimi-balance");
