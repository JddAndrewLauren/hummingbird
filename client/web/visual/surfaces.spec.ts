import { expect, test, type Page } from "@playwright/test";
import { NAV_BAR_OVERFLOW } from "../src/shell/nav-bar";
import { SCREEN_LABELS, SCREENS as SCREEN_ORDER, type Screen } from "../src/shell/screens";

// The visual gate's one spec. Two jobs, deliberately separated:
//
// 1. CAPTURE. Write a PNG per screen x width x theme into `visual/.captures/`
//    for a human to review for clipping, overlap, broken wrapping and
//    sticky/scroll glitches. Viewport-sized, not `fullPage`: the shell is
//    `height: 100dvh; overflow: hidden` (`.hb-shell`), so the document is
//    exactly one viewport tall on every project and `fullPage` can only add
//    what is not really there. Under the phone project's `isMobile` emulation
//    it did exactly that — Chromium reported a 1048px content height for an
//    844px page whose body, shell and window all measured 844, and every
//    long-screen capture carried 200px of dead space below the nav bar.
//    There is no committed golden and no pixel diff:
//    this project has no baseline history, and a pixel-diff gate with nobody to
//    arbitrate it produces noise, not findings. The captures are the
//    deliverable; `/wrapup`'s visual phase reads them.
//
// 2. ASSERT the few things a machine can decide without a baseline — that
//    nothing overflows horizontally, that the brand tokens actually resolve,
//    that the theme switch reaches the page, and (#453) that the page
//    actually loaded the world `openApp` asked for. These fail the run.
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
  // poller-backed panes plus one quiet, device-local reachability answer.
  { name: "status", nav: "Status" },
  { name: "settings", nav: "Settings" },
] as const;

const THEMES = ["light", "dark"] as const;

/** `"kit"` is `?demo` — the design system's fixtures, what nine screens
 * photograph. `"board"` is `?demo=board` (#420), a seeded `TaskState` that
 * makes the screens take their REAL render path. `null` is no flag at all: the
 * honest empty states. */
type World = "kit" | "board" | null;

// World-identity markers (#453). `demoMode()` (`src/fixtures/demo-mode.ts`)
// maps every unrecognised `?demo=` spelling onto the kit world, so a typo in
// a URL here — or a stale arm in this file's own world dispatch — used to
// silently fall back to a world the caller never asked for, and every
// assertion downstream still held because it never checked WHICH world
// loaded. These two strings exist only in their own world's fixture: the kit
// world's hero item (`demo-data.ts`'s `ION-118`, always the "top pick" since
// its stage is `in_progress`) and the board world's `@computer` column
// heading (`demo-task-state.ts` — the kit world's `DemoItem` has no
// `context` at all, so this string cannot appear there by construction).
// Checked in both directions, on purpose — a one-directional check (kit
// string present) passes for a page that loaded neither world, e.g. a
// silent 404 or a blank shell.
const KIT_ONLY_TEXT = "Rewrite the sweeper's Gmail adapter";
const BOARD_ONLY_TEXT = "@computer";

/** Which world's marker(s) the page currently shows — `"both"` and `null`
 * (no marker, i.e. "none") are both failures of the instrument itself, named
 * rather than collapsed into a boolean, so a broken dispatch reads as what
 * it is. */
async function loadedWorld(page: Page): Promise<World | "both"> {
  const hasKit = (await page.getByText(KIT_ONLY_TEXT).count()) > 0;
  const hasBoard = (await page.getByRole("heading", { name: BOARD_ONLY_TEXT }).count()) > 0;
  if (hasKit && hasBoard) return "both";
  if (hasKit) return "kit";
  if (hasBoard) return "board";
  return null;
}

/** Fails the run, naming both worlds, when the page did not load the world
 * `openApp` asked for. This is the instrument #453 exists to add: without
 * it, `demoMode()`'s fallback-to-kit behaviour (or a stale arm in the
 * `page.goto` dispatch below) leaves every later assertion in this file
 * photographing whichever world it silently got, still green. */
async function assertWorldLoaded(page: Page, asked: World) {
  const got = await loadedWorld(page);
  expect(
    got,
    `openApp asked for the "${asked ?? "none"}" world but the page loaded ` +
      `"${got ?? "none"}" (kit marker "${KIT_ONLY_TEXT}" ` +
      `${got === "kit" || got === "both" ? "present" : "absent"}, board marker ` +
      `"${BOARD_ONLY_TEXT}" heading ${got === "board" || got === "both" ? "present" : "absent"})`,
  ).toBe(asked);
}

/** The app resolves `light | dark | system` onto `[data-theme]` from
 * `hb.theme` (see `src/theme/`). Seeding the key before the first paint is
 * what avoids capturing a flash of the other theme. */
async function openApp(page: Page, theme: (typeof THEMES)[number], world: World) {
  await page.addInitScript((value) => {
    window.localStorage.setItem("hb.theme", value);
  }, theme);
  // Take the speech API away before the app loads (#379). NOT a convenience:
  // **headless Chromium 151.0.7922.34 crashes the renderer when
  // `SpeechRecognition.available()` is called.** Measured, both ways, same
  // build, same origin: headless -> `page.evaluate: Target crashed`;
  // `headless: false` -> `"downloadable"`. Without this, every capture-popover
  // case here died on the click that opens it, because mounting the capture box
  // is what fires the capability probe.
  //
  // Deleting the constructor is not pixel-neutral: a real headed browser
  // resolves `"downloadable"` to `setup-required` and renders #381's setup
  // mic, but this deletion pins the capability at `unsupported` instead,
  // which renders nothing. The gate accepts that gap because none of the
  // states past "nothing renders" are photographable here anyway (no live
  // mic, no on-device model download) — which is why this belongs in the
  // harness rather than as a `navigator.webdriver` check in product code,
  // where it would be a browser bug shaping the app.
  //
  // Nothing in-page could defend against this: a renderer crash is not
  // catchable from the page that provokes it. If the gate ever runs headed,
  // drop this and photograph the real `setup-required` arm.
  await page.addInitScript(() => {
    delete (globalThis as { SpeechRecognition?: unknown }).SpeechRecognition;
    delete (globalThis as { webkitSpeechRecognition?: unknown }).webkitSpeechRecognition;
  });
  await page.goto(world === "kit" ? "/?demo" : world === "board" ? "/?demo=board" : "/");
  // The shell paints before the wasm core is ready (the core's status is a
  // label, not a gate), so waiting on the nav rail is enough — and waiting
  // on the core would hang on a machine with no authority to reach.
  await expect(page.getByRole("navigation")).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  // #453: prove the URL above actually resolved to the world asked for,
  // before any caller's assertions build on that assumption.
  await assertWorldLoaded(page, world);
}

/** The five screens the phone's bottom bar cannot hold, by their nav name.
 * Imported from the app's own partition rather than restated here, so the
 * spec cannot drift from what the bar actually shows: add a screen and it
 * lands in the sheet in both places at once, or in neither. */
const PHONE_OVERFLOW_NAV = new Set(NAV_BAR_OVERFLOW.map((screen) => SCREEN_LABELS[screen]));

async function show(page: Page, nav: string, projectName: string) {
  // On the phone project five of the nine screens live behind "More"
  // (`src/shell/nav-bar.ts`) and are not in the DOM until the sheet is open.
  // The other three projects take the untouched path — they render the rail,
  // where every screen is one click.
  //
  // The sheet is scoped to the DIALOG, not the navigation landmark. It is a
  // sibling of the `<nav>` rather than a child of it, deliberately: inside the
  // landmark its five destinations, its close button and the theme toggle were
  // all announced as part of "Surfaces", and the last two are not navigation at
  // all (`NavBar.tsx`'s header). So a landmark-scoped query cannot reach these
  // five, and scoping to the dialog is not a workaround but the accurate
  // question.
  if (projectName === "phone" && PHONE_OVERFLOW_NAV.has(nav)) {
    await page.getByRole("navigation").getByRole("button", { name: "More", exact: true }).click();
    const sheet = page.getByRole("dialog", { name: "More surfaces" });
    await expect(sheet).toBeVisible();
    await sheet.getByRole("button", { name: nav, exact: true }).click();
    return;
  }
  // `exact`: since #304 the rail's wordmark is itself a way home, labelled
  // "hummingbird — go to Now and refresh" — inside the same navigation
  // landmark, so a substring match on "Now" resolves to two buttons and every
  // Now-routed case fails in strict mode. Each nav item's name is its
  // `aria-label`, which is exactly the label, so exact matching is right for
  // all of them.
  await page.getByRole("navigation").getByRole("button", { name: nav, exact: true }).click();
}

/** #481's Recall overlay: opens it through the header's Search button — the
 * one trigger mounted on every project (`shell/Header.tsx`'s button is
 * `isPhone`-independent, unlike the rail's magnifier and the phone More
 * sheet's own entry, which #480 wired as three more paths to the identical
 * `open`/`onClose` state). `exact` matters: the rail's own magnifier and the
 * More sheet's entry are both labelled "Search everything", a different
 * string, so this is unambiguous without it — kept anyway for the same
 * reason `show()` above keeps it on every nav button. */
async function openRecall(page: Page) {
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Recall" })).toBeVisible();
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

/** No horizontal overflow at any width — and be clear about how little that
 * proves. `App.tsx`'s root is `overflow: hidden`, so content wider than the
 * shell is CLIPPED rather than extending `documentElement.scrollWidth`: this
 * assertion mostly measures the thing that cannot happen. It caught the rules
 * editor's 137px, which escaped the clip anyway, so it is not useless — but
 * "90 screens pass" means "90 screens do not scroll sideways", NOT "90 screens
 * are usable at 390px". A row whose title ellipsises to two characters passes
 * this and is unreadable; that is why `ItemRow` wraps on the phone form
 * (`responsive.css`) on the strength of a human reading the captures, not of
 * this returning 0.
 *
 * There is a stronger assertion available — compare each scroll container's
 * own `scrollWidth` against its `clientWidth` — and it is deliberately not
 * written yet: it wants its own pass over which containers are legitimately
 * scrollable. One pixel of slack for sub-pixel rounding. */
async function expectNoHorizontalOverflow(page: Page) {
  const overflow = await page.evaluate(() => {
    const doc = document.documentElement;
    return doc.scrollWidth - doc.clientWidth;
  });
  expect(overflow).toBeLessThanOrEqual(1);
}

// #454: the BOARD world's own nine-screen capture pass, alongside the KIT
// pass above. Unlike the kit pass — which only proves the nav rendered and
// nothing overflowed — this one asserts something SPECIFIC about the seeded
// content, one assertion per screen, so a world change that produces eight
// blank screens cannot pass green.
//
// The per-screen assertions live here, keyed by the app's own `Screen` type
// (`Record<Screen, …>` rather than an array), so a screen added to
// `shell/screens.ts` without deciding what its board capture proves is a
// compile error rather than a silently-skipped screen — the same discipline
// `SCREEN_LABELS` already applies to nav names, and the reason this registry
// imports `SCREENS`/`Screen` from the app rather than restating the list.
//
// Routes and Alerts are KIT-ONLY, deliberately asserted as such rather than
// skipped: neither screen reads `TaskState` at all (`RoutesScreen` and
// `AlertsScreen` each take only `demo: DemoData | null`, never `task`), so
// under `?demo=board` both always render the same honest empty state an
// unseeded device would. Asserting that specific text — not merely "the
// screen renders" — is what would force this registry to change the day
// either screen is wired to the board seed instead of leaving the gap
// silently papered over.
type ScreenAssertion = (page: Page) => Promise<void>;

const BOARD_ASSERTIONS: Record<Screen, ScreenAssertion> = {
  now: async (page) => {
    await expect(page.getByRole("heading", { name: "@computer" })).toBeVisible();
    await expect(page.getByRole("alert")).toHaveCount(2);
  },
  triage: async (page) => {
    // `demo-task-state.test.ts` pins `triageInbox` at 17 and `grillingItems`
    // at 1 — `TriageScreen`'s real branch (taken because `demo` is null
    // under `?demo=board`) renders `triageProcessQueue`'s combined header,
    // `${capturedCount} captured · ${grillingCount} grilling`, never the kit
    // branch's "swept every 15m" wording nor the old "N unsorted" phrasing.
    await expect(page.getByText("17 captured · 1 grilling")).toBeVisible();
  },
  routes: async (page) => {
    await expect(page.getByRole("heading", { level: 2, name: "No routes yet" })).toBeVisible();
  },
  alerts: async (page) => {
    await expect(page.getByRole("heading", { name: "Live" })).toBeVisible();
    await expect(page.getByText("Nothing is ringing")).toBeVisible();
  },
  rules: async (page) => {
    // The absence check is the acceptance criterion's own bullet, not a
    // nicety: `RulesScreen` renders "Loading rules…" whenever either of
    // `rules`/`kindRegistry` is null, and only a synchronously-seeded board
    // (the board world's `task` is a `useState` lazy initializer, never
    // fetched) never shows it.
    await expect(page.getByText("Loading rules…")).toHaveCount(0);
    await expect(page.getByText("3 rules · default-deny")).toBeVisible();
  },
  done: async (page) => {
    // `DONE_SEEDS` is 6 long; `DoneScreen`'s "not read yet" only shows while
    // `task.done` is null, which the board seed never is.
    await expect(page.getByText("6 done")).toBeVisible();
  },
  ledger: async (page) => {
    // frontier (12) + triageInbox (17) + grillingItems (1) + done (6) + the
    // archived-only seeds (3) = 39 — the Ledger's pool is every live list
    // `demo-task-state.ts` builds, not just three of the four.
    await expect(page.getByText("39 ever · derived, not recorded")).toBeVisible();
  },
  status: async (page) => {
    // Ten poller-backed panes — `docs/SURFACES.md`'s own count, made
    // executable: one `kimi-balance/v1` gauge, five `github-hummingbird/v1`
    // workflow rows, three `uptime/v1` service rows, plus one quiet,
    // device-local `reachability` answer. `RankedRegion` gives every pane
    // its own toggle button (`aria-expanded`, collapsed or open) and no
    // other per-pane hook — no `role="region"`, no test id — so that
    // attribute, scoped to the page's one `<main>` landmark, is what gets
    // counted.
    await expect(page.getByRole("main").locator("[aria-expanded]")).toHaveCount(10);
  },
  settings: async (page) => {
    // `boundTripsBinding`'s key — the one binding row #452 added that
    // neither world had before. Bindings render only once the real core
    // reports `status === "ready"` (board mode reads `task.bindings`
    // because `demo` is null, and `SettingsScreen` gates that branch on the
    // live core status, not on anything the fixture controls), which is a
    // real async worker boot rather than fixture latency — hence the longer
    // timeout than the rest of this file needs.
    await expect(page.getByText("trips-calendar")).toBeVisible({ timeout: 15_000 });
  },
};

for (const theme of THEMES) {
  test.describe(`${theme} theme`, () => {
    for (const screen of SCREENS) {
      test(`${screen.name} renders and captures`, async ({ page }, testInfo) => {
        await openApp(page, theme, "kit");
        await show(page, screen.nav, testInfo.project.name);
        if (screen.name === "rules") {
          await openFirstRuleEditor(page);
        }
        // The rule editor is the ONE surface knowingly left overflowing at
        // 390 (137px over when the phone project was added) — its condition
        // rows are a dense grid of selects that needs its own design pass,
        // deferred out of this work rather than half-done inside it. Named
        // here as a single screen rather than skipped as a whole test, so the
        // capture is still taken and the exemption stays visible and narrow;
        // every other screen at 390 is held to the same bar as desktop.
        // Delete this branch, not the assertion, when that pass lands.
        const rulesExempt = screen.name === "rules" && testInfo.project.name === "phone";
        if (!rulesExempt) {
          await expectNoHorizontalOverflow(page);
        }
        await page.screenshot({
          path: `visual/.captures/${screen.name}-${testInfo.project.name}-${theme}.png`,
          fullPage: false,
        });
      });
    }

    for (const screenId of SCREEN_ORDER) {
      test(`board: ${screenId} renders and asserts the seed`, async ({ page }, testInfo) => {
        await openApp(page, theme, "board");
        await show(page, SCREEN_LABELS[screenId], testInfo.project.name);
        await BOARD_ASSERTIONS[screenId](page);
        await expectNoHorizontalOverflow(page);
        await page.screenshot({
          path: `visual/.captures/board-${screenId}-${testInfo.project.name}-${theme}.png`,
          fullPage: false,
        });
      });
    }

    test("the capture popover captures", async ({ page }, testInfo) => {
      // Its own state, not a screen: `shell/CapturePopover.tsx` renders over
      // whatever is showing, so no per-screen capture ever contains it. Opened
      // over Now, the widest thing behind it — a scrim that fails to cover, or
      // a card that overflows the 768 width, shows up here and nowhere else.
      await openApp(page, theme, "kit");
      await show(page, "Now", testInfo.project.name);
      await page.getByRole("button", { name: "New" }).click();
      await expect(page.getByRole("dialog")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/capture-popover-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
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
      // awkward real shape rather than a tidy one: 30 cards (12 frontier + 17
      // captured + the one fictional Grilling item `demo-task-state.ts`'s own
      // header flags as added after the 29 was measured), the no-context
      // bucket the biggest column and pinned last, and two columns over the
      // six-card cap showing `n more`.
      await openApp(page, theme, "board");
      await show(page, "Now", testInfo.project.name);
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

    // #481: the search overlay joins the registry as a photographed surface,
    // closing #331's "the busiest new surface shipping unphotographed"
    // finding. Board world only — the header's Search button (and every
    // other trigger #480 wired) is inert under `?demo` (`App.tsx`'s
    // `onSearch={demo ? undefined : requestSearchOpen}`), because the kit
    // world's `task` is a static fixture with no real `Core::search` to
    // answer against. `task.search` is itself a fixed seed even in board
    // mode (`fixtures/demo-task-state.ts`'s `recallRow` doc) — board's
    // `TaskState` is the lazy-initializer fixture `App.tsx`'s `task` always
    // resolves to, never the live store slice a real answer would land in —
    // so what is typed into the query field does not change which rows
    // render; these four captures are of the states themselves, not of a
    // live search loop.
    test("recall overlay: results listed", async ({ page }, testInfo) => {
      await openApp(page, theme, "board");
      await show(page, "Now", testInfo.project.name);
      await openRecall(page);
      const dialog = page.getByRole("dialog", { name: "Recall" });
      await dialog.getByRole("textbox").fill("the");
      // One row per group the seed carries — live, done and archived — so
      // this single capture proves all three render together, not just one.
      // Scoped to the dialog throughout: `b-f7`'s own board card sits behind
      // the overlay (Now's frontier renders it too) and carries the same
      // title, which a page-wide query would resolve ambiguously.
      await expect(
        dialog.getByRole("button", { name: /rewrite the backup script/i }),
      ).toBeVisible();
      await expect(dialog.getByText("Reply to the HOA about the fence colour")).toBeVisible();
      await expect(dialog.getByText("Chase the warranty claim")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/recall-results-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });
    });

    test("recall overlay: live result expanded with edit form", async ({ page }, testInfo) => {
      await openApp(page, theme, "board");
      await show(page, "Now", testInfo.project.name);
      await openRecall(page);
      const dialog = page.getByRole("dialog", { name: "Recall" });
      await dialog.getByRole("textbox").fill("the");
      await dialog.getByRole("button", { name: /rewrite the backup script/i }).click();
      // The live group is the one that gets `onTriage` (board mode's `demo`
      // is null, so `App.tsx` passes `handleTriage` through) — its expansion
      // is the only one of the three that offers Edit.
      await expect(dialog.getByRole("button", { name: "Edit" })).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/recall-live-expanded-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });
    });

    test("recall overlay: archived result expanded read-only", async ({ page }, testInfo) => {
      await openApp(page, theme, "board");
      await show(page, "Now", testInfo.project.name);
      await openRecall(page);
      const dialog = page.getByRole("dialog", { name: "Recall" });
      await dialog.getByRole("textbox").fill("the");
      await dialog.getByRole("button", { name: /chase the warranty claim/i }).click();
      // No `onTriage` for a `"done"`/`"archived"` group (`RecallRow`'s own
      // rule) — the expansion shows the record with no Edit affordance at
      // all, rather than a second read-only mode invented for this overlay.
      await expect(dialog.getByText(/created .* ago/)).toBeVisible();
      await expect(dialog.getByRole("button", { name: "Edit" })).toHaveCount(0);
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/recall-readonly-expanded-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });
    });

    test("recall overlay: empty query", async ({ page }, testInfo) => {
      await openApp(page, theme, "board");
      await show(page, "Now", testInfo.project.name);
      await openRecall(page);
      await expect(page.getByText("Type to search")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/recall-empty-query-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });
    });

    test("empty states capture", async ({ page }, testInfo) => {
      // No `?demo`: the honest empty states, which are what a new device
      // actually shows and which no fixture screen ever exercises.
      await openApp(page, theme, null);
      await show(page, "Now", testInfo.project.name);
      await expect(page.getByText("Nothing to start")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/now-empty-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
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
