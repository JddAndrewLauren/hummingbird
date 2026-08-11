// The one seam between real data and the design kit's fixtures.
//
// Gated twice, deliberately. `import.meta.env.DEV` is substituted with the
// literal `false` at build time, so the production bundle contains
// `if (false && …)` — demo mode cannot be switched on by a query string in
// production even if someone tries, and Rollup drops DEMO_DATA with the
// dead branch. The `?demo` check is what makes it opt-in during development,
// so `pnpm dev` still shows the honest empty states by default.

import { DEMO_DATA, type DemoData } from "./demo-data";
import { isDemoEnabled } from "./demo-mode";

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
