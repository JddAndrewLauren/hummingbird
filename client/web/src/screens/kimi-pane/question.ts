import type { QuestionDef } from "../questions/contract";
import { SNAPSHOT_KEY, SOURCE, kimiAnswer } from "./kimi";
import { KimiPaneExpanded } from "./KimiPaneExpanded";

/** "What's left of my Moonshot balance?" as the shell's registry sees it
 * (#313 over #245/ADR-0017, replacing #311's `placeholderQuestion` call).
 *
 * One subject, always — `SNAPSHOT_KEY` is both the one `context_snapshots`
 * row this source ever holds and this question's sentinel subject, returned
 * whether or not a poller has ever run: the never-polled state is a gap, per
 * `contract.ts`'s "a platform with no rows yet renders as a gap, never as
 * nothing." There is no unbound state to return instead — see `kimi.ts`'s
 * header for why this question has no per-device setup at all. */
export const kimiQuestion: QuestionDef = {
  label: "Kimi balance",
  surface: "status",
  sources: [SOURCE],
  subjects: () => [SNAPSHOT_KEY],
  answer: (_subjectKey, inputs) => kimiAnswer(inputs),
  Expanded: KimiPaneExpanded,
};
