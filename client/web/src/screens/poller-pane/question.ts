import type { QuestionDef } from "../questions/contract";
import { SOURCES, pollerAnswer, pollerSubjects } from "./poller";
import { PollerPaneExpanded } from "./PollerPaneExpanded";

/** "Is this poller writing on time?" (#775) — one pane per source
 * `poller.rs`'s `poller_sources` watches, always ranked: there is no
 * per-device binding here, so a source this device has not read from yet is
 * a gap (`bound-but-unacquired`), never `unbound` (`poller.ts`'s own
 * header). */
export const pollerQuestion: QuestionDef = {
  surface: "status",
  sources: SOURCES,
  subjects: pollerSubjects,
  answer: pollerAnswer,
  Expanded: PollerPaneExpanded,
};
