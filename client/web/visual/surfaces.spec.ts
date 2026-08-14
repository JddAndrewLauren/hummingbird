import { expect, test, type Page } from "@playwright/test";

// The visual gate's one spec. Two jobs, deliberately separated:
//
// 1. CAPTURE. Write a PNG per screen x width x theme into `visual/.captures/`
//    for a human to review for clipping, overlap, broken wrapping and
//    sticky/scroll glitches. There is no committed golden and no pixel diff:
//    this project has no baseline history, and a pixel-diff gate with nobody to
//    arbitrate it produces noise, not findings. The captures are the
//    deliverable; `/wrapup`'s visual phase reads them.
//
// 2. ASSERT the few things a machine can decide without a baseline — that
//    nothing overflows horizontally, that the brand tokens actually resolve,
//    and that the theme switch reaches the page. These fail the run.
//
// Everything renders in `?demo` mode: the fixtures are deterministic and
// populated, where real data on a dev machine is an empty mirror — the shell
// takes the authority's origin from its own (`src/worker/core.worker.ts`),
// and `vite dev` proxies `/api` to a local `wrangler dev` this run does not
// start, with no deployed authority behind it either (#95's H3 gate). The
// honest empty states are captured too, from the same screens without the
// flag.
//
// Since #420 there are two populated worlds, not one, because `?demo` cannot
// reach every surface. `?demo` is the KIT world (the design system's
// display-shaped fixtures) and drives the nine screens below. `?demo=board`
// is the BOARD world: a seeded `TaskState` that makes the screens take their
// real render path, which is the only way this gate can photograph Now's
// centre column at all — see the `now's columns` test.

const SCREENS = [
  { name: "now", nav: "Now" },
  { name: "triage", nav: "Triage" },
  { name: "routes", nav: "Routes" },
  { name: "alerts", nav: "Alerts" },
  { name: "rules", nav: "Rules" },
  // Done and the Ledger have no demo fixtures, so under `?demo` these two
  // photograph their "not read yet" holding state — a real state (the
  // round-trip between mount and the first answer) rather than a fixture
  // gap, and the populated rows are covered by their component tests.
  { name: "done", nav: "Done" },
  { name: "ledger", nav: "Ledger" },
  // #311/ADR-0017: the same `?demo` world drives the real Status region.
  // #313 landed the first poller-backed, non-gap pane (`kimi-balance/v1`,
  // banded "near" with a negative cash split); #314 landed the second
  // (`github-hummingbird/v1`, five workflow rows — one per band the pane can
  // produce — which is what makes the collapsed stack long enough to be
  // worth capturing at 768px); #315 landed the third (`uptime/v1`, three
  // service rows, all in quiet agreement). This capture therefore shows nine
  // poller-backed panes alongside the *one* still-honest gap pane that is
  // left — `reachability`, unpolled pending #316. Not permanent: that gap
  // goes when #316 lands.
  { name: "status", nav: "Status" },
  { name: "settings", nav: "Settings" },
] as const;

const THEMES = ["light", "dark"] as const;

/** `"kit"` is `?demo` — the design system's fixtures, what nine screens
 * photograph. `"board"` is `?demo=board` (#420), a seeded `TaskState` that
 * makes the screens take their REAL render path. `null` is no flag at all: the
 * honest empty states. */
type World = "kit" | "board" | null;

/** The app resolves `light | dark | system` onto `[data-theme]` from
 * `hb.theme` (see `src/theme/`). Seeding the key before the first paint is
 * what avoids capturing a flash of the other theme. */
async function openApp(page: Page, theme: (typeof THEMES)[number], world: World) {
  await page.addInitScript((value) => {
    window.localStorage.setItem("hb.theme", value);
  }, theme);
  await page.goto(world === "kit" ? "/?demo" : world === "board" ? "/?demo=board" : "/");
  // The shell paints before the wasm core is ready (the core's status is a
  // label, not a gate), so waiting on the nav rail is enough — and waiting
  // on the core would hang on a machine with no authority to reach.
  await expect(page.getByRole("navigation")).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
}

async function show(page: Page, nav: string) {
  // `exact`: since #304 the rail's wordmark is itself a way home, labelled
  // "hummingbird — go to Now and refresh" — inside the same navigation
  // landmark, so a substring match on "Now" resolves to two buttons and every
  // Now-routed case fails in strict mode. Each nav item's name is its
  // `aria-label`, which is exactly the label, so exact matching is right for
  // all of them.
  await page.getByRole("navigation").getByRole("button", { name: nav, exact: true }).click();
}

/** The Rules screen's own capture prep: open the first rule card's editor
 * so condition rows and the backtest panel — otherwise hidden behind an
 * "Edit" click (#140 review: backtest moved into the draft editor, so it
 * is never on-screen by default) — actually appear in the capture. Without
 * this the capture proves nothing about the acceptance criterion it exists
 * to exercise. */
async function openFirstRuleEditor(page: Page) {
  await page.getByRole("button", { name: "Edit" }).first().click();
  await page.getByRole("button", { name: "Backtest" }).first().click();
}

/** No horizontal overflow at any width. The layout wraps rather than using
 * media queries (`screens/layout.tsx`), so an element that refuses to shrink
 * shows up here and nowhere else — a real clipping bug, machine-decidable
 * without a golden. One pixel of slack for sub-pixel rounding. */
async function expectNoHorizontalOverflow(page: Page) {
  const overflow = await page.evaluate(() => {
    const doc = document.documentElement;
    return doc.scrollWidth - doc.clientWidth;
  });
  expect(overflow).toBeLessThanOrEqual(1);
}

for (const theme of THEMES) {
  test.describe(`${theme} theme`, () => {
    for (const screen of SCREENS) {
      test(`${screen.name} renders and captures`, async ({ page }, testInfo) => {
        await openApp(page, theme, "kit");
        await show(page, screen.nav);
        if (screen.name === "rules") {
          await openFirstRuleEditor(page);
        }
        await expectNoHorizontalOverflow(page);
        await page.screenshot({
          path: `visual/.captures/${screen.name}-${testInfo.project.name}-${theme}.png`,
          fullPage: true,
        });
      });
    }

    test("the capture popover captures", async ({ page }, testInfo) => {
      // Its own state, not a screen: `shell/CapturePopover.tsx` renders over
      // whatever is showing, so no per-screen capture ever contains it. Opened
      // over Now, the widest thing behind it — a scrim that fails to cover, or
      // a card that overflows the 768 width, shows up here and nowhere else.
      await openApp(page, theme, "kit");
      await show(page, "Now");
      await page.getByRole("button", { name: "New" }).click();
      await expect(page.getByRole("dialog")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/capture-popover-${testInfo.project.name}-${theme}.png`,
        fullPage: true,
      });
    });

    test("now's columns capture, at production's density", async ({ page }, testInfo) => {
      // #420, and the reason the board world exists. `NowScreen` branches to
      // `RealFrontier` only when `demo` is null, so everything ADR-0021
      // decided — the wrapping columns, the switchable axis, the Filter
      // panel, the unsorted captures as cards among the startable actions,
      // and #418's stranded-triage alert — was invisible to this gate from
      // the day it landed. Decision 8 recorded that; this closes it.
      //
      // The fixture mirrors production's measured spread
      // (`fixtures/demo-task-state.ts`), so what gets photographed is the
      // awkward real shape rather than a tidy one: 29 cards, the no-context
      // bucket the biggest column and pinned last, and two columns over the
      // six-card cap showing `n more`.
      await openApp(page, theme, "board");
      await show(page, "Now");
      // The board is up (a column heading the fixture guarantees) and the
      // alerts with it — waiting on both is what stops a capture of a
      // half-rendered screen.
      //
      // Two alerts, not one: Now says a stranded triage failure and a stranded
      // act failure on separate lines, because the store holds one result per
      // mutation KIND rather than one failure slot they take turns in, and the
      // board fixture seeds both. A `getByRole("alert")` here would be strict-
      // mode ambiguous — which is how this gate caught the second line
      // arriving, and worth keeping counted rather than loosened to `.first()`.
      await expect(page.getByRole("heading", { name: "@computer" })).toBeVisible();
      const alerts = page.getByRole("alert");
      await expect(alerts).toHaveCount(2);
      await expect(alerts.first()).toBeVisible();
      await expect(alerts.last()).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/now-columns-${testInfo.project.name}-${theme}.png`,
        fullPage: true,
      });
    });

    test("empty states capture", async ({ page }, testInfo) => {
      // No `?demo`: the honest empty states, which are what a new device
      // actually shows and which no fixture screen ever exercises.
      await openApp(page, theme, null);
      await show(page, "Now");
      await expect(page.getByText("Nothing to start")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/now-empty-${testInfo.project.name}-${theme}.png`,
        fullPage: true,
      });
    });
  });
}

test.describe("brand token bindings", () => {
  // Asserted against the tokens themselves, never a hardcoded hex — the
  // design system is the source of truth and a re-pull must be free to
  // change the values (CLAUDE.md, "The design system").
  test("the layout constants and accent resolve, and dark mode reaches the page", async ({
    page,
  }) => {
    await openApp(page, "light", "kit");

    const tokens = await page.evaluate(() => {
      const style = getComputedStyle(document.documentElement);
      return {
        rail: style.getPropertyValue("--rail-width").trim(),
        panel: style.getPropertyValue("--panel-width").trim(),
        content: style.getPropertyValue("--content-max").trim(),
        accent: style.getPropertyValue("--accent").trim(),
        page: style.getPropertyValue("--surface-page").trim(),
      };
    });
    // Present and non-empty is the assertion — an unresolved custom property
    // reads as "" and silently renders as nothing, which is the failure mode
    // worth catching. The specific values belong to the design system.
    for (const [name, value] of Object.entries(tokens)) {
      expect(value, `--${name} resolved`).not.toBe("");
    }
    expect(tokens.rail).toMatch(/px$/);

    const lightPage = await page.evaluate(
      () => getComputedStyle(document.body).backgroundColor,
    );

    await openApp(page, "dark", "kit");
    expect(await page.getAttribute("html", "data-theme")).toBe("dark");
    const darkPage = await page.evaluate(
      () => getComputedStyle(document.body).backgroundColor,
    );
    // The switch must actually repaint the page surface, not merely set an
    // attribute nothing reads.
    expect(darkPage).not.toBe(lightPage);
  });
});
