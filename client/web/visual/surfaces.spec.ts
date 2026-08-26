import { expect, test, type Page } from "@playwright/test";
import { NAV_BAR_OVERFLOW } from "../src/shell/nav-bar";
import { CAPTURE_TRIGGER_ID, RECALL_TRIGGER_ID } from "../src/shell/trigger-ids";
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
// Bare `?demo` renders the BOARD world: the fixtures are deterministic and
// populated, where real data on a dev machine is an empty mirror — the shell
// takes the authority's origin from its own (`src/worker/core.worker.ts`),
// and `vite dev` proxies `/api` to a local `wrangler dev` this run does not
// start, with no deployed authority behind it either (#95's H3 gate). The
// honest empty states are captured too, from the same screens without the
// flag.
//
// Since #420 there are two populated worlds, not one, because the kit world
// cannot reach every surface — #455 flipped which one is the default.
// Bare `?demo` (and every spelling but the kit's own) is the BOARD world: a
// seeded `TaskState` that makes the screens take their real render path,
// which is the only way this gate can photograph Now's centre column at all
// — see the `now's columns` test — and it is what the nine screens below
// photograph, being the primary pass since #454 proved it complete. `?demo=kit`
// is the KIT world (the design system's display-shaped fixtures); nothing in
// this file photographs it any more, `demo-data.ts`'s own header and the
// design system's own kit review are its cover now.

const THEMES = ["light", "dark"] as const;

/** `"kit"` is `?demo=kit` — the design system's fixtures. No test in this file
 * opens it any more (#455): the nine-screen loop, the capture popover and the
 * brand-token pass all moved to `"board"`, which is bare `?demo` (and every
 * spelling but the kit's own) — a seeded `TaskState` (#420) that makes the
 * screens take their REAL render path. `null` is no flag at all: the honest
 * empty states. `"kit"` stays in this union for a caller that adds one back,
 * but `loadedWorld` can no longer actually detect it (see `KIT_ONLY_TEXT`'s
 * own note below) — a `"kit"` open today fails `assertWorldLoaded` by
 * reporting the got-world as `"none"`, not as a diagnosed kit/board
 * mismatch. */
type World = "kit" | "board" | null;

// World-identity markers (#453). `demoMode()` (`src/fixtures/demo-mode.ts`)
// maps every unrecognised `?demo=` spelling onto the board world (#455 —
// onto the kit before it), so a typo in a URL here — or a stale arm in this
// file's own world dispatch — could silently fall back to a world the caller
// never asked for, and every assertion downstream still held because it
// never checked WHICH world loaded. These two strings were meant to exist
// only in their own world's fixture: the kit world's hero item (`demo-data.
// ts`'s `ION-118`, always the "top pick" since its stage is `in_progress`)
// and the board world's `@computer` column heading (`demo-task-state.ts` —
// the kit world's `DemoItem` has no `context` at all, so this string cannot
// appear there by construction). That symmetry broke one-sided at #456:
// `NowScreen` — the landing screen this check reads — deleted its kit-only
// hero card and "Also startable" list, so `KIT_ONLY_TEXT` no longer renders
// on any screen this file's `openApp` can reach; `hasKit` below is
// permanently `false` in practice, dead instrumentation rather than a live
// marker. `BOARD_ONLY_TEXT` is unaffected — Now's real frontier still
// renders `@computer` off the board seed. Checked in both directions on
// purpose regardless — a one-directional check (kit string present) passes
// for a page that loaded neither world, e.g. a silent 404 or a blank shell.
const KIT_ONLY_TEXT = "Rewrite the sweeper's Gmail adapter";
const BOARD_ONLY_TEXT = "@computer";

/** Which world's marker(s) the page currently shows — `"both"` and `null`
 * (no marker, i.e. "none") are both failures of the instrument itself, named
 * rather than collapsed into a boolean, so a broken dispatch reads as what
 * it is. `"kit"`/`"both"` are unreachable in practice today (`KIT_ONLY_TEXT`'s
 * own note above) — kept rather than deleted so this function still reports
 * correctly the day a kit render path exists again. */
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
 * it, `demoMode()`'s unrecognised-spelling fallback (or a stale arm in the
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
  // "board" goes through bare `/?demo` — the default spelling since #455 —
  // rather than the explicit `/?demo=board`, so the gate actually exercises
  // `demoMode()`'s fallback, not just the one spelling that always worked.
  await page.goto(world === "kit" ? "/?demo=kit" : world === "board" ? "/?demo" : "/");
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
 * `open`/`onClose` state).
 *
 * Addressed by the trigger's own id, not by its accessible name. All three
 * triggers now say "Search everything" — one gesture, one name — so no name
 * query can pick the header's button out: at the three desktop widths
 * `NavRail` is mounted alongside `Header` and its magnifier wears the
 * identical name, which is a strict-mode violation whether the match is
 * exact or not. Nor can a landmark separate them the way `show()` above
 * separates the nav: `Header.tsx`'s `<header>` is rendered INSIDE `<main>`,
 * and a `header` nested in `main` carries no `banner` role at all, so
 * `getByRole("banner")` matches nothing here. `RECALL_TRIGGER_ID` is the one
 * unambiguous handle on this button — imported from the component that owns
 * it (the same id `RecallOverlay` measures to hang itself under), so a
 * rename moves both ends at once. */
async function openRecall(page: Page) {
  await page.locator(`#${RECALL_TRIGGER_ID}`).click();
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

/** The rules LIST state, captured separately from the editor-open one above.
 * Not a nicety: `demo-data.ts`'s `rule-unranked-severity` is last in the seed
 * specifically to photograph its wrapping badge row, and the editor this file
 * opens on rule 1 — condition rows plus an expanded backtest — pushes that
 * fourth card off the bottom of every viewport (at 1440 only the badge's top
 * edge survived; at 1024 and below the whole card did not). So the badge is
 * scrolled into view and shot before the editor opens. The screen's own
 * per-project capture below stays the editor-open state, unchanged.
 *
 * The badge is addressed by the same `/^Unranked severity —/` prefix
 * `RulesScreen.test.tsx`'s #374 test uses, not by the full sentence: the copy
 * is one `UNRANKED_SEVERITY_COPY` constant in `RulesScreen.tsx`, and pinning
 * its whole text from out here would be a second copy to keep in step. */
async function captureRulesList(page: Page, projectName: string, theme: string) {
  const badge = page.getByText(/^Unranked severity —/);
  await expect(badge).toHaveCount(1);
  await badge.scrollIntoViewIfNeeded();
  await expect(badge).toBeInViewport();
  await page.screenshot({
    path: `visual/.captures/rules-list-${projectName}-${theme}.png`,
    fullPage: false,
  });
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

// #454's BOARD world nine-screen capture pass — since #455 the only one, and
// the primary gate for the nine screens below. It asserts something SPECIFIC
// about the seeded content, one assertion per screen, so a world change that
// produces eight blank screens cannot pass green — a bar the kit-world pass
// this replaced never cleared, since that one only proved the nav rendered
// and nothing overflowed.
//
// The per-screen assertions live here, keyed by the app's own `Screen` type
// (`Record<Screen, …>` rather than an array), so a screen added to
// `shell/screens.ts` without deciding what its board capture proves is a
// compile error rather than a silently-skipped screen — the same discipline
// `SCREEN_LABELS` already applies to nav names, and the reason this registry
// imports `SCREENS`/`Screen` from the app rather than restating the list.
//
// Alerts is KIT-ONLY, deliberately asserted as such rather than skipped: it
// does not read `TaskState` at all — since #457 it calls its own dev-gated
// `demoData()` directly (`AlertsScreen.tsx`), taking no `demo` prop from
// `App.tsx` — so under `?demo` (the board world) it always renders the same
// honest empty state an unseeded device would. Asserting that specific text
// — not merely "the screen renders" — is what would force this registry to
// change the day the screen is wired to the board seed instead of leaving
// the gap silently papered over. Its KIT-populated render (fixture rule
// cards) is no longer photographed by this gate at all since #455 retired
// the kit-world capture pass; that half of the gap is covered by
// `AlertsScreen.test.tsx` (#457) instead, per `docs/SURFACES.md`.
//
// Routes was the second such screen and is gone (#624): `ProjectsScreen`
// replaced it, reads `TaskState` on every world and takes no `demo` prop at
// all. Unlike Done and the Ledger it does NOT photograph a holding state
// under `?demo`: `useFrontierWiring` requests `projects` app-wide the moment
// the core is ready, on every world, so an unseeded world captures a real,
// empty answer ("No projects yet") rather than "not read yet". Under the
// board world it photographs the real grid off the fixture's seeded projects
// — which is why `demo-task-state.ts` seeds three (its departure 4) rather
// than production's measured zero.
type ScreenAssertion = (page: Page) => Promise<void>;

/** Settings' `Standing questions` section, minus its leftovers group — the
 * roster's own headings and nothing else (#714). Counted rather than named
 * anywhere in this file: the labels are the core's, and spelling one here
 * would put back the per-client copy the roster exists to delete. */
function questionHeadings(page: Page) {
  return page
    .locator("#standing-questions")
    .getByRole("heading", { level: 3 })
    .filter({ hasNotText: "Other settings rows" });
}

/** The roster's own disclosure buttons (#715) — one per question, plus the
 * leftovers group when there is one. Located inside the questions' headings
 * so the leftovers' button is excluded the same way `questionHeadings` does
 * it, and no question's label is spelled in this file. */
function questionRows(page: Page) {
  return questionHeadings(page).getByRole("button");
}

const BOARD_ASSERTIONS: Record<Screen, ScreenAssertion> = {
  now: async (page) => {
    // #456: the kit world's hero card and "Also startable" list are gone —
    // `NowScreen` no longer takes a `demo` prop at all, so there is no
    // branch left that could render either, on any world. A positive
    // absence check rather than an inferred one: the block really went,
    // not merely moved off this gate's board-only capture pass.
    await expect(page.getByText("Also startable")).toHaveCount(0);
    await expect(page.getByRole("heading", { name: "@computer" })).toBeVisible();
    await expect(page.getByRole("alert")).toHaveCount(2);
    // #455: the nav's triage/alerts badges derive from the store now
    // (`App.tsx`'s `navCounts`), not from `DemoData` — asserted once here
    // rather than on every screen, since the nav is identical chrome
    // wherever it renders. 17 is `triageInbox.length` (`demo-task-state.
    // test.ts` pins it, and the `triage` assertion below reads the same
    // number off the screen itself); 2 is `liveWriteFailureCount`'s count of
    // the seed's two stranded write failures, the same two the `alert` role
    // count above already proves are live. Both Triage and Alerts sit on the
    // phone bar's primary four (`nav-bar.ts`), so this locator resolves
    // identically on every project, not only desktop.
    await expect(
      page.getByRole("navigation").getByRole("button", { name: "Triage", exact: true }),
    ).toContainText("17");
    await expect(
      page.getByRole("navigation").getByRole("button", { name: "Alerts", exact: true }),
    ).toContainText("2");
  },
  triage: async (page) => {
    // #456: `TriageScreen` no longer takes a `demo` prop, so the kit
    // world's fixture card list and its "swept every 15m" meta cannot
    // render on any world — a positive absence check for the deleted meta
    // wording, not an inference from the presence check below.
    await expect(page.getByText(/swept every 15m/)).toHaveCount(0);
    // `demo-task-state.test.ts` pins `triageInbox` at 17 and `grillingItems`
    // at 1 — `TriageScreen`'s one render path renders `triageProcessQueue`'s
    // combined header, `${capturedCount} captured · ${grillingCount}
    // grilling`, never the old "N unsorted" phrasing.
    await expect(page.getByText("17 captured · 1 grilling")).toBeVisible();
  },
  projects: async (page) => {
    // A seeded card by NAME, not merely "the grid rendered": the fixture's
    // departure 4 exists precisely so this cannot pass on an empty screen.
    await expect(page.getByRole("heading", { level: 3, name: "House repairs" })).toBeVisible();
    // Archived is hidden until the toggle, so the third seed must NOT be here.
    await expect(page.getByRole("heading", { level: 3, name: "Sell the old bike" })).toHaveCount(0);
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
    // 4 since #374 added a rule at an out-of-vocabulary severity so this
    // gate photographs its wrapping badge row — which is the `rules-list-*`
    // capture, not the `rules-*` one: the open editor pushes that fourth
    // card past the bottom of every viewport.
    await expect(page.getByText("4 rules · default-deny")).toBeVisible();
  },
  done: async (page) => {
    // `DONE_SEEDS` is 6 long; `DoneScreen`'s "not read yet" only shows while
    // `task.done` is null, which the board seed never is.
    await expect(page.getByText("6 done")).toBeVisible();
  },
  ledger: async (page) => {
    // frontier (14, including departure 5's two `@homework` items) +
    // triageInbox (17) + grillingItems (1) + done (6) + the archived-only
    // seeds (3) = 41 — the Ledger's pool is every live list
    // `demo-task-state.ts` builds, not just three of the four.
    await expect(page.getByText("41 ever · derived, not recorded")).toBeVisible();
  },
  status: async (page) => {
    // Ten poller-backed panes — `docs/SURFACES.md`'s own count, made
    // executable: one `kimi-balance/v1` gauge, five `github-hummingbird/v1`
    // workflow rows, three `uptime/v1` service rows, plus one quiet,
    // device-local `reachability` answer. `StatusBoard` gives every pane one
    // tile and no other per-pane hook — no `role="region"`, no test id — so
    // `data-tile-tone`, which both tile arms carry, is what counts them.
    await expect(page.getByRole("main").locator("[data-tile-tone]")).toHaveCount(10);
    // **Nine toggles, not ten.** An answered reachability pane has nothing
    // beneath its headline to disclose, so its tile is drawn without a
    // toggle rather than with one that opens onto an empty card; the seed
    // answers that pane, so exactly one tile is missing its `aria-expanded`.
    // A tenth appearing here means the empty expansion came back.
    await expect(page.getByRole("main").locator("[aria-expanded]")).toHaveCount(9);
    // Both grids are labelled and counted, so a group that silently lost its
    // panes cannot pass as a board that simply has fewer tiles.
    await expect(page.getByText(/^infra · \d+ subjects?$/)).toBeVisible();
    await expect(page.getByText(/^capture & context sources · \d+ subjects?$/)).toBeVisible();
  },
  settings: async (page) => {
    // #456: `SettingsScreen` no longer takes a `demo` prop, so the kit
    // world's "Show acked alerts" switch and its inert "Mirror" section
    // cannot render on any world — a positive absence check, not an
    // inference from the presence check below.
    await expect(page.getByRole("heading", { name: "Mirror", exact: true })).toHaveCount(0);
    await expect(page.getByRole("switch", { name: "Show acked alerts" })).toHaveCount(0);
    // #714: the section is a list of QUESTIONS now, each with its binding
    // rows nested under it, and the list is the core's roster rather than
    // anything this client declares. Counted rather than named: the count is
    // the question vocabulary's size, so a question that stopped being
    // listed fails here instead of passing as a screen that merely looks
    // similar — and no question's label is spelled in this file, which is
    // the whole point of the roster.
    // Filtered, not a bare count: the section also draws an **Other
    // settings rows** heading whenever a live row belongs to no question,
    // so a seeded binding with an unwritable key would fail this for a
    // reason that has nothing to do with the roster.
    // Bindings render only once the real core reports `status === "ready"`
    // (`SettingsScreen` gates that branch on the live core status, not on
    // anything the fixture controls), which is a real async worker boot
    // rather than fixture latency — hence the longer timeout.
    await expect(questionHeadings(page)).toHaveCount(10, { timeout: 15_000 });

    // #715: every row is shut by default, so the board's Settings capture
    // opens all ten — which is the frame this pass photographed before the
    // rows became disclosures, and the only way to photograph the binding
    // fields at all. Opened by position, never by name.
    const rows = questionRows(page);
    for (let index = 0; index < 10; index += 1) {
      await rows.nth(index).click();
    }
    await expect(page.locator("#standing-questions").getByRole("switch")).toHaveCount(10);
    // `boundTripsBinding`'s key — the one binding row #452 added that
    // neither world had before, and now the proof that opening a row really
    // reveals what it holds.
    await expect(page.getByText("trips-calendar")).toBeVisible();

    // #707, review round 1: the two SharedWorker diagnostic-journal
    // controls previously sat below the fold in every capture this file
    // takes (`fullPage: false`), so nothing here — human review or
    // automated assertion — ever actually saw them. A real Playwright
    // `toBeVisible()` check is a stronger proof than the screenshot below
    // ever was: it fails if the element is not rendered or is CSS-hidden,
    // regardless of scroll position.
    await expect(page.getByRole("button", { name: "Download diagnostics", exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Clear diagnostics", exact: true })).toBeVisible();
  },
};

for (const theme of THEMES) {
  test.describe(`${theme} theme`, () => {
    // #455: this was two loops — a kit-world pass that only proved the nav
    // rendered and nothing overflowed, and the board-world pass added by
    // #454 that actually asserts the seeded content. The flip made the board
    // pass primary, so the kit loop is gone rather than kept as a second,
    // weaker capture of the same nine screens; `openFirstRuleEditor` and the
    // 390px rule-editor exemption move here from it unchanged.
    for (const screenId of SCREEN_ORDER) {
      test(`${screenId} renders and asserts the seed`, async ({ page }, testInfo) => {
        await openApp(page, theme, "board");
        await show(page, SCREEN_LABELS[screenId], testInfo.project.name);
        await BOARD_ASSERTIONS[screenId](page);
        // Checked on the LIST state, before the rule editor (if any) opens
        // below — every screen, rules included, is held to the same bar
        // here. Splitting this from the editor-open check below is what
        // keeps the rules list itself covered at 390 even though the editor
        // that opens over it is exempt (#374 was a rule-CARD overflow at
        // 1024/768, a different state than either of these, still caught by
        // this same call at those widths).
        await expectNoHorizontalOverflow(page);
        if (screenId === "rules") {
          // Before the editor opens, and its own PNG — see `captureRulesList`
          // for why the editor-open capture below cannot stand in for it.
          await captureRulesList(page, testInfo.project.name, theme);
          await openFirstRuleEditor(page);
          // The rule editor is the ONE surface knowingly left overflowing at
          // 390 (137px over when the phone project was added) — its
          // condition rows are a dense grid of selects that needs its own
          // design pass, deferred out of this work rather than half-done
          // inside it. Only the editor-OPEN state is exempt, and only at
          // 390: the list-state check above already ran, unconditionally,
          // for every project including phone. Delete this branch, not the
          // assertion above, when that pass lands.
          const rulesEditorExempt = testInfo.project.name === "phone";
          if (!rulesEditorExempt) {
            await expectNoHorizontalOverflow(page);
          }
        }
        await page.screenshot({
          path: `visual/.captures/${screenId}-${testInfo.project.name}-${theme}.png`,
          fullPage: false,
        });
      });
    }

    test("an open Status tile captures", async ({ page }, testInfo) => {
      // Its own state, not a screen: the board's per-screen capture is the
      // all-compact board, so the shape only an open tile has — a card
      // spanning two grid columns, with a pane's own body inside it — is
      // photographed nowhere else. It is also where the board can overflow:
      // an expanded tile is the widest thing on the surface.
      await openApp(page, theme, "board");
      await show(page, "Status", testInfo.project.name);
      const tiles = page.getByRole("main").locator("[aria-expanded]");
      await tiles.first().click();
      await expect(tiles.first()).toHaveAttribute("aria-expanded", "true");
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/status-expanded-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });
    });

    test("the capture popover captures", async ({ page }, testInfo) => {
      // Its own state, not a screen: `shell/CapturePopover.tsx` renders over
      // whatever is showing, so no per-screen capture ever contains it. Opened
      // over Now, the widest thing behind it — a scrim that fails to cover, or
      // a card that overflows the 768 width, shows up here and nowhere else.
      await openApp(page, theme, "board");
      await show(page, "Now", testInfo.project.name);
      // #455: a name query for "New" was unambiguous under the kit fixture,
      // but the board seed's real item titles ("Renew the car insurance",
      // "Fit the new tap washer") both contain that substring — the same
      // reason `openRecall` below addresses its trigger by id rather than by
      // name. `CAPTURE_TRIGGER_ID` is that same unambiguous handle, already
      // imported for the exclusivity test further down.
      await page.locator(`#${CAPTURE_TRIGGER_ID}`).click();
      await expect(page.getByRole("dialog")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/capture-popover-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });
    });

    test("the capture popover's context list", async ({ page }, testInfo) => {
      // A state inside a state, and unphotographed until now for the reason
      // the capture popover itself was: the Context suggestions used to be a
      // native `<datalist>`, browser chrome that takes none of the design
      // tokens and that a screenshot cannot even see. Since the list is ours
      // (`components/forms/Combobox.tsx`) it is a real surface — a popup
      // card over a form inside a dialog — and it is the one place in the
      // app where three elevations stack. The phone form is where that goes
      // wrong first: the popup is absolutely positioned inside a card that
      // is already near the viewport's bottom.
      await openApp(page, theme, "board");
      await show(page, "Now", testInfo.project.name);
      await page.locator(`#${CAPTURE_TRIGGER_ID}`).click();
      await expect(page.getByRole("dialog")).toBeVisible();
      await page.getByRole("button", { name: "Show context suggestions" }).click();
      await expect(page.getByRole("listbox")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/capture-context-list-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });
    });

    test("projects: the dossier behind the grid", async ({ page }, testInfo) => {
      // #624. The per-screen board capture above only ever photographs the
      // GRID — `ProjectsScreen` opens the dossier on local state, so the
      // second of its two levels is unreachable without a click, and would
      // otherwise ship unphotographed at every width and in both themes.
      // What this proves is that the frame, the two-column skeleton and the
      // back affordance survive the phone form — exactly where a `TwoColumn`
      // is most likely to overflow — and that the aside's four POPULATED
      // record cards do too: the fixture seeds real links and a real
      // destination/notes pair onto "House repairs", so the row layouts
      // (ellipsised URL, the move pair, Edit, Remove, all inside the narrow
      // `Aside`) are in every capture rather than a "Reading links…"
      // placeholder, and #630's archive confirm step is photographed on its
      // own below.
      //
      // The centre column is now Now's board filtered to this project, so
      // these captures are also the board's second surface: the axis
      // switcher minus the Project button, the columns at a project's own
      // (much smaller) card count, and — in the third capture — the
      // selected-item slot open above them. The fixture assigns three
      // frontier items and one capture to "House repairs" precisely so this
      // reads as a board rather than a single card.
      await openApp(page, theme, "board");
      await show(page, "Projects", testInfo.project.name);
      await page.getByRole("heading", { level: 3, name: "House repairs" }).click();
      await expect(page.getByRole("heading", { level: 2, name: "House repairs" })).toBeVisible();
      await expect(page.getByRole("button", { name: "All projects" })).toBeVisible();
      // The links card must actually be populated in the shot — a regression
      // back to the unanswered-read placeholder would otherwise photograph
      // an empty region and still pass.
      // `exact` because the properties card's own link is named "Open GitHub
      // repo", which the default substring match also picks up.
      await expect(page.getByRole("link", { name: "Repo", exact: true })).toBeVisible();
      // Same guarantee for the properties card's repo link glyph, which is
      // the only thing on that card carrying the derived URL.
      await expect(page.getByRole("link", { name: "Open GitHub repo" })).toBeVisible();
      // Same guarantee for the Route card (#627).
      await expect(page.getByLabel("Destination")).toHaveValue(
        "The deck is rebuilt, permitted and passes inspection.",
      );
      // Same guarantee for the board: this project's own items are on it,
      // and another project's are not — a filter regression would otherwise
      // photograph the whole frontier and still pass.
      // By card, not by text: the board fixture also seeds a failed act
      // naming this same item, so its stranded-write alert carries the title
      // too and a bare text locator resolves to two elements.
      await expect(page.getByRole("button", { name: /^Fit the new tap washer/ })).toBeVisible();
      await expect(
        page.getByRole("button", { name: /^Clear the gutters before the storms/ }),
      ).toBeVisible();
      await expect(page.getByRole("button", { name: /^Sort the shed shelves/ })).toHaveCount(0);
      // And the chrome that makes it a board, minus the axis that would be
      // one column here.
      // `exact`, both of them: the nav rail's own "Projects" entry and this
      // screen's cards otherwise match the substring.
      await expect(
        page.getByRole("button", { name: "Context", exact: true, pressed: true }),
      ).toBeVisible();
      await expect(page.getByRole("button", { name: "Project", exact: true })).toHaveCount(0);
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/projects-dossier-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });

      // ADR-0021 decision 7 on this surface: selecting a card expands the
      // item's panel ABOVE the columns, which stay standing under it. A
      // second capture rather than overwriting the first, so both the closed
      // and the open state stay reviewable — and this is the one that
      // photographs the panel inside a project's narrower centre column.
      // `b-f1` carries two steps in the fixture, so the panel's checklist is
      // populated here rather than empty.
      await page.getByRole("button", { name: /^Fit the new tap washer/ }).click();
      await expect(page.getByText("Turn off the stopcock")).toBeVisible();
      await expect(page.getByRole("button", { name: "Close item detail" })).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/projects-dossier-slot-open-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });

      // Shut again, so the archive capture below is the ordinary dossier and
      // not the dossier with a panel open across it.
      await page.getByRole("button", { name: "Close item detail" }).click();

      // #630: the archive card's confirm dialog — the last placeholder this
      // aside carried. "House repairs" holds live items in the fixture's own
      // Ledger seed, so the count named here is non-zero, not the honest-
      // but-untested "no live items" branch.
      await page.getByRole("button", { name: "Archive project" }).click();
      await expect(page.getByText(/Archiving takes \d+ live items? down with it\./)).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/projects-dossier-archive-confirm-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });
    });

    test("projects: a dossier whose board is empty", async ({ page }, testInfo) => {
      // The board's empty state, which must say "in this project" rather
      // than Now's global "Nothing to start" — a project with nothing
      // startable is not a claim about the whole frontier.
      //
      // The archived "Sell the old bike" is the fixture's project with no
      // items at all, which is also honest: archiving cascades onto a
      // project's live items (ADR-0030 decision 5), so an archived project
      // with an empty board is the state the app really produces. Reaching
      // it needs the grid's Show-archived toggle, so this capture covers the
      // archived dossier too — the `archived` badge included.
      await openApp(page, theme, "board");
      await show(page, "Projects", testInfo.project.name);
      await page.getByRole("switch", { name: "Show archived" }).click();
      await page.getByRole("heading", { level: 3, name: "Sell the old bike" }).click();
      await expect(page.getByRole("heading", { level: 2, name: "Sell the old bike" })).toBeVisible();
      await expect(page.getByText("Nothing startable in this project")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/projects-dossier-empty-board-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });
    });

    test("now's columns capture, at production's density", async ({ page }, testInfo) => {
      // #420, and the reason the board world exists. Until #456, `NowScreen`
      // branched to `RealFrontier` only when `demo` was null, so everything
      // ADR-0021 decided — the columns (packed into lanes since the width
      // became a measured fact, `screens/frontier-lanes.ts`), the switchable
      // axis, the Filter panel, the unsorted captures as cards among the
      // startable actions, and #418's stranded-triage alert — was invisible
      // to this gate from the day it landed. Decision 8 recorded that; this
      // closes it. (#456 later deleted that branch entirely — `NowScreen`
      // renders the board unconditionally now (`FrontierBoard.tsx` since the
      // project dossier began sharing it) — but the board world's
      // populated render below is still the only one this gate photographs.)
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
    // finding. Board world only — until #456, the header's Search button
    // (and every other trigger #480 wired) was inert under `?demo=kit`
    // (`App.tsx`'s `onSearch={demo ? undefined : requestSearchOpen}`); #456
    // deleted that ternary, so the trigger is unconditional now. What still
    // confines this to the board world is `task` itself: `demoTaskState()`
    // (`fixtures/demo.ts`) returns a seed only for the board spelling, so
    // under `?demo=kit` `task` is `liveTask`, the real store slice, with no
    // seeded `search` answer — opening Recall there sends a real request to
    // whatever `Core::search` the live worker resolves, not a fixed result.
    // `task.search` is itself a fixed seed even in board
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
      // The live group is the one that gets `onTriage` (`App.tsx` passes
      // `handleTriage` through unconditionally since #456) — its expansion
      // is the only one of the three that offers Edit. `ItemPanel` gates its
      // fields behind `editing` (`components/domain/ItemPanel.tsx`'s
      // `showFields`), which only the Edit click flips — asserting Edit is
      // merely visible captures the same read-only record the archived row's
      // own capture already shows, so this must actually press it and prove
      // the form itself is open before the screenshot.
      await dialog.getByRole("button", { name: "Edit" }).click();
      await expect(dialog.getByLabel("Title")).toHaveValue(
        "Rewrite the backup script so it prunes old snapshots",
      );
      await expect(dialog.getByRole("button", { name: "Save" })).toBeVisible();
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

    // No screenshot: this asserts a rule, not a surface. It lives here
    // because the rule is `App.tsx` state and there is no `App.test.tsx` —
    // the two openers are the only thing that enforces it, and the four
    // captures above are already the place Recall is driven end-to-end
    // through a real shell. Both dialogs are `aria-modal` at one z-index with
    // no focus trap between them, so "both open" was a reader tabbing between
    // two things that each claimed to own the window.
    //
    // **Driven by `press("Enter")`, and that is the honest gesture, not a
    // workaround for Playwright.** Each overlay lays a full-window scrim at
    // `zIndex: 40` over the header, so with either one open neither trigger
    // can be CLICKED at all — and the `/` and `c` hotkeys both refuse an
    // editable target, which is where focus lands when either opens. Reaching
    // the second overlay means tabbing out to the trigger behind the scrim
    // and pressing it: keyboard focus is what the missing trap fails to
    // contain, so the keyboard is also the only way in. That is exactly the
    // path FINAL-GATE recorded, and the one this closes.
    test("recall overlay: each opener closes the other overlay", async ({ page }, testInfo) => {
      await openApp(page, theme, "board");
      await show(page, "Now", testInfo.project.name);
      const capture = page.getByRole("dialog", { name: "New capture" });
      const recall = page.getByRole("dialog", { name: "Recall" });

      await page.locator(`#${CAPTURE_TRIGGER_ID}`).click();
      await expect(capture).toBeVisible();
      await page.locator(`#${RECALL_TRIGGER_ID}`).press("Enter");
      await expect(recall).toBeVisible();
      // The new fact: the popover is gone rather than sitting underneath.
      await expect(capture).toBeHidden();

      // And back the other way. Assertion only — nothing is typed and no
      // capture is submitted, so this leaves no fixture behind.
      await page.locator(`#${CAPTURE_TRIGGER_ID}`).press("Enter");
      await expect(capture).toBeVisible();
      await expect(recall).toBeHidden();
    });

    // No screenshot: a rule, not a surface, and it lives here for the same
    // reason the exclusivity test above does — it is `App.tsx` state, there
    // is no `App.test.tsx`, and the two halves it joins are in different
    // files. `Combobox` publishes "my list is open"
    // (`components/forms/combobox-open.ts`), `escape-claimants.ts` ranks it
    // above the popover, and neither one alone can show that ONE Escape
    // closes the list and leaves the draft standing. Keyboard-only
    // throughout, which is also the path with no pointer to dismiss with.
    test("the context list takes the first Escape, the popover the second", async ({ page }, testInfo) => {
      await openApp(page, theme, "board");
      await show(page, "Now", testInfo.project.name);
      const capture = page.getByRole("dialog", { name: "New capture" });
      const list = page.getByRole("listbox");
      const field = page.getByRole("combobox", { name: "Context" });

      await page.locator(`#${CAPTURE_TRIGGER_ID}`).click();
      await expect(capture).toBeVisible();

      // Into the list from the field, with no pointer: ArrowDown opens it
      // browsing, a second moves the active option, Enter commits it.
      await field.focus();
      await field.press("ArrowDown");
      await expect(list).toBeVisible();
      await field.press("ArrowDown");
      await field.press("Enter");
      await expect(field).toHaveValue("@computer");
      await expect(list).toHaveCount(0);

      await field.press("ArrowDown");
      await expect(list).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(list).toHaveCount(0);
      await expect(capture).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(capture).toHaveCount(0);
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

      // #714: Settings on a device that has bound nothing. The roster is
      // now the only place a question can be seen when its own pane is
      // quiet (ADR-0034), so the frame where NOTHING is set is the one
      // worth photographing — and it is the frame the `?demo` world above
      // cannot reach, since that world hand-authors bindings.
      await show(page, "Settings", testInfo.project.name);
      await expect(questionHeadings(page)).toHaveCount(10, { timeout: 15_000 });
      // #715: every row starts shut, so this frame is also the collapsed
      // roster — ten questions, no toggles, no fields.
      await expect(questionRows(page)).toHaveCount(10);
      await expect(page.locator("#standing-questions").getByRole("switch")).toHaveCount(0);
      // #707: the same proof as the board-world capture above, over the
      // REAL core rather than the fixture — a device that has bound
      // nothing still gets a reachable diagnostic journal.
      await expect(page.getByRole("button", { name: "Download diagnostics", exact: true })).toBeVisible();
      await expect(page.getByRole("button", { name: "Clear diagnostics", exact: true })).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/settings-empty-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });

      // #715's two remaining frames, driven through the REAL core rather
      // than a fixture: the `?demo` world overrides `TaskState`, so a click
      // there would change nothing. Here the write really enqueues, really
      // overlays, and really comes back — which is what makes the "off"
      // capture a photograph of the feature rather than of a prop.
      //
      // The first row, never a named one: the labels are the core's, and
      // spelling one here would put back the per-client copy #714 deleted.
      const firstRow = questionRows(page).first();
      await firstRow.click();
      await expect(firstRow).toHaveAttribute("aria-expanded", "true");
      const asked = page.locator("#standing-questions").getByRole("switch");
      await expect(asked).toHaveCount(1);
      await expect(asked).toBeChecked();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/settings-question-expanded-${testInfo.project.name}-${theme}.png`,
        fullPage: false,
      });

      // Switched off: the row says so, and — the part worth photographing —
      // it keeps saying so with the row shut again, which is the only way an
      // off question is discoverable at all (ADR-0034's consequences).
      await asked.click();
      await expect(asked).not.toBeChecked();
      await firstRow.click();
      await expect(firstRow).toHaveAttribute("aria-expanded", "false");
      await expect(page.locator("#standing-questions").getByText("off")).toHaveCount(1);
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/settings-question-off-${testInfo.project.name}-${theme}.png`,
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
    await openApp(page, "light", "board");

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

    await openApp(page, "dark", "board");
    expect(await page.getAttribute("html", "data-theme")).toBe("dark");
    const darkPage = await page.evaluate(
      () => getComputedStyle(document.body).backgroundColor,
    );
    // The switch must actually repaint the page surface, not merely set an
    // attribute nothing reads.
    expect(darkPage).not.toBe(lightPage);
  });
});
