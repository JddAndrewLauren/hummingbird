import type { QuestionDef } from "../questions/contract";
import { SOURCE, uptimeAnswer, uptimeSubjects } from "./uptime";
import { UptimePaneExpanded } from "./UptimePaneExpanded";

/** "Is the authority, the web origin and the runner answering HTTP right
 * now?" (#315 over ADR-0017 decisions 2/3/4/6, replacing #311's
 * `placeholderQuestion` call).
 *
 * One pane per service `server/uptime-probe` has ever written a row for —
 * `authority`, `web`, `runner` today, one source spanning two platforms
 * (`uptime.ts`'s own header). The never-polled sentinel subject is still
 * returned when nothing has ever been read, on `contract.ts`'s "a platform
 * with no rows yet renders as a gap, never as nothing." */
export const uptimeQuestion: QuestionDef = {
  label: "Uptime",
  surface: "status",
  sources: [SOURCE],
  subjects: uptimeSubjects,
  answer: uptimeAnswer,
  Expanded: UptimePaneExpanded,
};
