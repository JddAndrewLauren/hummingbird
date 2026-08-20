// Demo mode renders fixtures instead of the app's real data, so the shell can
// be checked before any task data exists. It is gated twice — see demo.ts.
//
// There are two demo worlds, because they answer different questions.
//
// `?demo=kit` is the **kit** world: the design system's hand-authored
// fixtures (`DemoData`), a display-shaped model of items, captures, alerts
// and routes. It is what the design system's kit is compared against.
//
// Bare `?demo` (and every other spelling — `?demo=1`, `?demo=board`,
// `?demo=anything`) is the **board** world (#420): a seeded `TaskState`, the
// same shape the sync engine publishes, which makes the screens take their
// REAL render path with fictional data in it, and is what the nine screens
// photograph. It exists because the kit world cannot reach Now's centre
// column at all — until #456, `NowScreen` branched to `RealFrontier` only
// when `demo` was null, so the frontier's columns, the captures among them
// and every control on them were invisible to the visual gate from the day
// they landed (ADR-0021 decision 8). `DemoItem` could not express them
// either: it carries no `context` and no `energy`, having been written
// before those were axes.
//
// *Amended 2026-08-20 (#455): before this, bare `?demo` meant the kit world
// and `?demo=board` was the one recognised spelling of the board. The flip
// is this file: the board is now the default, and the kit needs the one
// exact spelling, `?demo=kit`, to reach it. See ADR-0021 decision 8's own
// amendment.*
//
// *Amended 2026-08-20 (#456): `NowScreen` deleted its `demo` prop and the
// branch above with it — it renders `RealFrontier` unconditionally now, on
// every world. The board world's reason for existing (above) is historical;
// it still stands, since #456 did not restore the kit world's reach into
// Now's centre column.*

export type DemoMode = "kit" | "board";

/** Which demo world the query string asks for, or `null` for none.
 *
 * `?demo=kit` is the one recognised spelling of the kit world; every other
 * spelling — bare `?demo`, `?demo=1`, `?demo=board`, `?demo=anything` — is
 * the board world, the default since #455. */
export function demoMode(search: string): DemoMode | null {
  const params = new URLSearchParams(search);
  if (!params.has("demo")) {
    return null;
  }
  return params.get("demo") === "kit" ? "kit" : "board";
}

/** The kit world specifically — `demoData()`'s gate, kept as its own name
 * because "is the design kit showing" is what every existing caller means. */
export function isDemoEnabled(search: string): boolean {
  return demoMode(search) === "kit";
}
