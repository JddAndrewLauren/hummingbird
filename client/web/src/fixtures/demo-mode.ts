// Demo mode renders fixtures instead of the app's real data, so the shell can
// be checked before any task data exists. It is gated twice — see demo.ts.
//
// There are two demo worlds, because they answer different questions.
//
// `?demo` is the **kit** world: the design system's hand-authored fixtures
// (`DemoData`), a display-shaped model of items, captures, alerts and routes.
// It is what nine screens photograph.
//
// `?demo=board` is the **board** world (#420): a seeded `TaskState`, the same
// shape the sync engine publishes, which makes the screens take their REAL
// render path with fictional data in it. It exists because the kit world
// cannot reach Now's centre column at all — `NowScreen` branches to
// `RealFrontier` only when `demo` is null, so the frontier's columns, the
// captures among them and every control on them were invisible to the visual
// gate from the day they landed (ADR-0021 decision 8). `DemoItem` could not
// express them either: it carries no `context` and no `energy`, having been
// written before those were axes.

export type DemoMode = "kit" | "board";

/** Which demo world the query string asks for, or `null` for none.
 *
 * `?demo=board` is the one recognised value; every other spelling — bare
 * `?demo`, `?demo=1`, `?demo=anything` — stays the kit world it has always
 * been, so no existing link or gate invocation changes meaning. */
export function demoMode(search: string): DemoMode | null {
  const params = new URLSearchParams(search);
  if (!params.has("demo")) {
    return null;
  }
  return params.get("demo") === "board" ? "board" : "kit";
}

/** The kit world specifically — `demoData()`'s gate, kept as its own name
 * because "is the design kit showing" is what every existing caller means. */
export function isDemoEnabled(search: string): boolean {
  return demoMode(search) === "kit";
}
