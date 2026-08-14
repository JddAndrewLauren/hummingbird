// The one seam between real data and the design kit's fixtures.
//
// Gated twice, deliberately. `import.meta.env.DEV` is substituted with the
// literal `false` at build time, so the production bundle contains
// `if (false && …)` — demo mode cannot be switched on by a query string in
// production even if someone tries, and Rollup drops DEMO_DATA with the
// dead branch. The `?demo` check is what makes it opt-in during development,
// so `pnpm dev` still shows the honest empty states by default.

import { DEMO_DATA, type DemoData } from "./demo-data";
import { demoMode, isDemoEnabled } from "./demo-mode";
import { DEMO_TASK_STATE } from "./demo-task-state";
import type { TaskState } from "../store/store";

export type {
  DemoAlert,
  DemoBinding,
  DemoCalendar,
  DemoCapture,
  DemoData,
  DemoItem,
  DemoRoute,
  DemoRule,
} from "./demo-data";

export function demoData(): DemoData | null {
  if (!import.meta.env.DEV) {
    return null;
  }
  return isDemoEnabled(window.location.search) ? DEMO_DATA : null;
}

/** The board world's seeded `TaskState` (#420), or `null` for every other
 * mode. Behind the same double gate `demoData` is: substituted to
 * `if (false && …)` in a production build, so Rollup drops `DEMO_TASK_STATE`
 * with the dead branch and no fixture item can reach a real device.
 *
 * Deliberately mutually exclusive with `demoData()` — `?demo=board` returns a
 * state here and `null` there, which is the whole mechanism: a null `demo`
 * prop is what makes `NowScreen` take its `RealFrontier` branch, and this is
 * the data that branch then reads. */
export function demoTaskState(): TaskState | null {
  if (!import.meta.env.DEV) {
    return null;
  }
  return demoMode(window.location.search) === "board" ? DEMO_TASK_STATE : null;
}
