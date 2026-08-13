import { placeholderQuestion } from "../questions/placeholder";

// "Is the authority, the web origin and the runner answering HTTP right
// now?" (ADR-0017 decisions 3/4/6, #315's `uptime/v1` — one credential-free
// prober spanning all three, manifest-driven). Replaced wholesale once that
// poller exists; see `placeholder.tsx`'s header for why this is a call, not
// a hand-shaped `QuestionDef` of its own.
export const uptimeQuestion = placeholderQuestion("Uptime", "uptime");
