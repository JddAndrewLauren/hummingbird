import { placeholderQuestion } from "../questions/placeholder";

// "Are hummingbird's own workflows healthy?" (ADR-0017 decision 2, #314's
// `github-hummingbird/v1` — one question, one poller, one pane per
// scheduled workflow once it exists). Replaced wholesale then; see
// `placeholder.ts`'s header for why this is a call, not a hand-shaped
// `QuestionDef` of its own.
export const githubQuestion = placeholderQuestion("GitHub workflows", "github-hummingbird");
