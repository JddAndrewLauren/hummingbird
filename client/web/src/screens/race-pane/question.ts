import type { QuestionDef } from "../questions/contract";
import { SOURCE, raceAnswer, raceSubjects } from "./race";
import { RacePaneExpanded } from "./RacePaneExpanded";

/** "When is the next race?" as the shell's registry sees it (#119 over
 * #245, reading #266's lane).
 *
 * **The first question with a genuinely variable number of subjects**: one
 * pane per series named in the `race-series` binding, so following another
 * series is an edit in Settings rather than a code change — ADR-0015's "a
 * question registers once and emits 0..N panes", and this issue's own
 * acceptance criterion. An unbound question still emits its one sentinel
 * subject, because the setup prompt is how anyone discovers the question
 * exists. */
export const raceQuestion: QuestionDef = {
  label: "When is the next race",
  surface: "now",
  sources: [SOURCE],
  subjects: raceSubjects,
  answer: raceAnswer,
  Expanded: RacePaneExpanded,
};
