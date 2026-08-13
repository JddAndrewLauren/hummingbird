import type { QuestionDef } from "../questions/contract";
import { SNAPSHOT_KEY, SOURCE, wasteAnswer } from "./waste";
import { WastePaneExpanded } from "./WastePaneExpanded";

/** "Which cans go out?" as the shell's registry sees it (#120 over #245).
 *
 * One subject, always — the single collection this address has. It is
 * returned even while the question is unbound, because the setup prompt is
 * how anyone discovers the question exists; a pane that vanished until it
 * was configured could never be configured. */
export const wasteQuestion: QuestionDef = {
  label: "Which cans go out",
  surface: "now",
  sources: [SOURCE],
  subjects: () => [SNAPSHOT_KEY],
  answer: (_subjectKey, inputs) => wasteAnswer(inputs),
  Expanded: WastePaneExpanded,
};
