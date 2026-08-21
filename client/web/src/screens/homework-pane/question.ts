import type { QuestionDef } from "../questions/contract";
import { HomeworkPaneExpanded } from "./HomeworkPaneExpanded";
import { homeworkAnswer, homeworkSubjects } from "./homework";

/** "What's my next piece of homework?" as the shell's registry sees it
 * (#675 over #245).
 *
 * **The first question with no source at all.** `sources` is empty and
 * there are no `calendarRequests`: the subject is the operator's own items,
 * which the wiring already puts on `QuestionInputs.items`. Nothing polls
 * for this pane and nothing binds it — which is also why its one subject is
 * a fixed sentinel rather than something a binding names. */
export const homeworkQuestion: QuestionDef = {
  label: "What's my homework",
  surface: "now",
  sources: [],
  subjects: homeworkSubjects,
  answer: homeworkAnswer,
  Expanded: HomeworkPaneExpanded,
};
