import type { QuestionDef } from "../questions/contract";
import { ReachabilityPaneExpanded } from "./ReachabilityPaneExpanded";
import { SUBJECT_KEY, reachabilityAnswer } from "./reachability";

// The one Status answer only this device can give (#316). It deliberately
// registers no source: authority reachability is inferred from the existing
// sync cycle and persisted locally, never polled through a second lane.
export const reachabilityQuestion: QuestionDef = {
  label: "This device",
  surface: "status",
  sources: [],
  subjects: () => [SUBJECT_KEY],
  answer: (_subjectKey, inputs) => reachabilityAnswer(inputs),
  Expanded: ReachabilityPaneExpanded,
};
