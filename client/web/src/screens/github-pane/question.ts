import type { QuestionDef } from "../questions/contract";
import { SOURCE, githubAnswer, githubSubjects } from "./github";
import { GithubPaneExpanded } from "./GithubPaneExpanded";

/** "Are hummingbird's own workflows healthy?" as the shell's registry sees
 * it (#314 over #245/ADR-0017 decision 2, replacing #311's
 * `placeholderQuestion` call).
 *
 * One pane per scheduled workflow `server/github-status` has ever written a
 * row for — the first question on either surface whose subject list comes
 * from which snapshot keys exist rather than from a binding or a fixed
 * sentinel (`github.ts`'s own header). The never-polled sentinel subject is
 * still returned when nothing has ever been read, on `contract.ts`'s "a
 * platform with no rows yet renders as a gap, never as nothing." */
export const githubQuestion: QuestionDef = {
  surface: "status",
  sources: [SOURCE],
  subjects: githubSubjects,
  answer: githubAnswer,
  Expanded: GithubPaneExpanded,
};
