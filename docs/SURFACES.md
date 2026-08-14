# Surface registry

The registry of this repo's visual surfaces: what renders, where its code
lives, which toolset gates it, and at what matrix. `/wrapup`'s visual
verification phase reads this file, maps the session diff against it, and
runs the listed gate for **each affected surface, scoped to the affected
screens**.

A **brand-token change affects every surface**, not just the screens a diff
touched — see "Brand-token bindings" below.

---

## Surface: web

The only built surface, in **two forms**: desktop, and — since the mobile pass
— a phone form below 640px. `hb.twinion.net` (ADR-0006), a PWA offline shell
served from Cloudflare Workers static assets, deployed from `main` by
`.github/workflows/deploy-client.yml` — live since 2026-08-10.

**The breakpoint is 640px**, defined in exactly two places:
`src/shell/breakpoints.ts` (`PHONE_MAX_WIDTH_PX`) and the literal inside every
`@media` in `src/shell/responsive.css` — CSS cannot read a custom property
from a media query and there is no PostCSS plugin here.
`src/shell/responsive-breakpoint.test.ts` reads the stylesheet's source and
pins the two equal. 640 and not 768 because 768 is already a documented
desktop state in the matrix below ("the context panel has wrapped below the
column"), so moving the nav there would silently redefine a width this gate
already photographs.

**How the phone form is expressed** — the split is hard, and by kind:

- **CSS classes** (`src/shell/responsive.css`) for the pure-layout elements:
  the shell row, the scroll container, the header and its title, and the four
  screen skeletons — plus a partial split on `ItemRow`, whose class carries
  only the phone wrap. Everything in `src/` styles through inline `style={{}}`
  objects, and at equal importance a stylesheet rule loses to an element's own
  `style` attribute; `!important` is the one mechanism that outranks it, for
  shorthands as much as longhands. So those elements' style objects were
  *deleted* rather than supplemented — the alternative was an `!important` on
  every declaration in the phone block, and on anything that later had to
  override one. The file's single surviving `!important` (the 16px input rule,
  against iOS focus-zoom) is that same mechanism used once, where deleting was
  not available: the token file is a copied mirror of the design project and
  `Input`/`Textarea`/`Select` set the `font` shorthand inline.
- **`src/shell/useIsPhone.ts`** (a `matchMedia` hook) only where the DOM tree
  itself differs: the nav rail versus the bottom bar, and `CapturePopover`'s
  JS-measured anchor.

| | |
| --- | --- |
| **Code root** | `client/web/src/` |
| **Screens** | `screens/*.tsx` — Now, Triage, Routes, Alerts, Rules, Done, Ledger, Status, Settings |
| **Now's aside** | `screens/questions/RankedRegion.tsx` — ADR-0015's ranked standing-question region (#245), and the landmark is named **"Standing questions"** for it (#401, ADR-0021 decision 6: it read `Context` long after the context tile stopped being there, and the word was needed for the centre column's grouping axis) — plus each question's own expanded pane (`screens/waste-pane/`, `screens/weekend-pane/` #122, `screens/vacation-pane/` #121, `screens/race-pane/` #119 — the first question emitting one pane *per subject*, so the aside's height varies with the `race-series` binding). It replaced the calendar context tile, so the aside now *grows* with the number of questions: `screens/layout.tsx`'s `Aside` caps at `100dvh` and scrolls itself, which is a change every screen with an aside inherits (Now, Settings, Alerts, Routes). **On the phone form all three of those properties are undone** (`.hb-aside` in `shell/responsive.css`): sticky + `100dvh` + `overflow-y: auto` on a full-width panel below the column would make a nested scroll region, and the reachability problem they solve does not exist once the panel is stacked in the flow — the page scrolls its whole height. |
| **Now's centre column** | `screens/NowScreen.tsx`'s `RealFrontier` — the frontier in **wrapping columns** (`screens/FrontierColumns.tsx`, grouped by a switchable axis — Context, Project, Size or Energy — over the pure `screens/frontier-columns.ts`; #402, ADR-0021), then Blocked. The **unsorted captures are cards in those same columns** rather than a section of their own: `TaskState.triageInbox` is appended to the ordered frontier before grouping, so each capture lands in whichever column the live axis puts it in (the no-value one until something sets that field) and sits **under** that column's startable actions, marked by its `triage` `StageBadge` — the same stage vocabulary the Triage screen's rows use, and not a fourth meaning for colour. There is now **no independent scroll container in the centre column at all** — the shell's one container scrolls the page — which the columns already assumed: they wrap onto more lines instead of scrolling sideways, and no column overflows on its own (each caps at six cards with an `n more` toggle). Colour the card itself introduces encodes urgency and nothing else (the stage chip, the priority label and the **size and energy chips** are `ItemRow`'s, and stage is one of the three things the design system lets colour mean; since #446 size and energy are coloured glyphs drawn from their own ramp, which is the cost ADR-0024 decision 2 accepts against ADR-0021 — an amber *mark* on a card can mean due-soon or normal-size, and only the card's own colour still means urgency alone), and the urgency is stated in words too — in text colours, not the swatch's, which is a contrast requirement rather than a taste one. Since #403 each column header collapses **in place** (shrink-to-fit, so neighbours reflow rather than leaving a hole) and a Filter button opens a facet panel (context/size/energy/urgency, OR within a facet and AND across) with an `n of m shown` readout; the axis and the collapsed set persist device-locally via `screens/frontier-prefs.ts`, the filter selection deliberately does not, and changing the axis clears the collapsed set. Since #404 **selecting a card is not a takeover**: the item panel expands *above* the columns, which stay mounted and visible under it, with the source card marked (`aria-current` plus an accent fill) and the panel scrolled into view — `RealFrontier` used to return the panel *instead of* the frontier. Selecting a **capture** fills that same slot with `TriageRow` forced open, so S13/#111's "two editors are never open at once" holds by construction — one slot, one editor — and the captures' cards stay on the board whichever kind is open. Both are now the **same component** (`components/domain/ItemPanel.tsx`) in its two modes: `"triage"` stands the fields open and ends in the two promotions, `"detail"` reads as a record until **Edit** reveals the identical fields and saves them through `Core::triage` with `destination: null` (#122's stage-agnostic edit) — before that fold, a minted action's own fields were reachable nowhere in the app. Clicking the open card again closes it, and Escape closes the panel from anywhere (`shell/capture-hotkey.ts`'s `closesItemDetail`, which yields to the capture popover). Since #418 a **failed write is stated above the columns, naming the item** (`screens/write-failure.ts`), for the case the slot made reachable: the editor that would otherwise wear the failure is unmounted the moment the reader closes the panel. There are **two such lines, not one** — a failed triage and a failed act are separate results in the store (`lastTriage`, `lastAct`), so a shared slot would let one failure hide the other. Each is suppressed while the editor that owns it is what the slot holds — for a triage, either editor, since detail mode says its own failures once it can edit — so no result is ever stated twice; the act line is *not* suppressed for an open capture, whose `TriageRow` renders no act failure though its checkmark issues one. |
| **Status** | `screens/StatusScreen.tsx` (#311, ADR-0017) — the same `RankedRegion` as Now's aside, instantiated a second time (`surface="status"`) as a single-column screen rather than an aside; three questions read real pollers (`screens/kimi-pane/` #313, `screens/github-pane/` #314, `screens/uptime-pane/` #315), and `screens/reachability-pane/` #316 answers from this device's persisted authority-sync history, with no poller or source of its own. |
| **Shell** | `shell/Header.tsx`, `shell/NavRail.tsx` (desktop) / `shell/NavBar.tsx` (phone — four screens plus a More sheet, partitioned by `shell/nav-bar.ts`; `App.tsx` mounts exactly one, since two navigation landmarks break the spec's strict-mode `getByRole("navigation")`), `shell/ShellMeta.tsx` (the core-state and build-version lines, in the rail's footer and at the foot of the More sheet — on a phone that sheet and Settings are the only two places the build version is reachable), `shell/CapturePopover.tsx` (the capture box, over any screen), `shell/UpdateBanner.tsx` (the "new version — reload" strip, under the header), `screens/layout.tsx`, `shell/responsive.css` |
| **Components** | `components/{core,forms,domain,feedback}/` — the 16-component library |
| **Toolset** | Playwright (`client/web/playwright.config.ts`, `client/web/visual/`) |
| **Command** | `cd client/web && pnpm visual` |
| **Captures** | `client/web/visual/.captures/` (gitignored) |

### Matrix

Four widths × two themes × twelve screen states, per run.

| Project | Width | What it proves |
| --- | --- | --- |
| `wide` | 1440 | Rail, content column and context panel side by side — the design target. |
| `boundary` | 1024 | The wrap point. 236px rail + 380px minimum column + 320px panel plus gaps lands within a few pixels of this, so it is where `screens/layout.tsx`'s `TwoColumn` decides. A layout regression shows here first. |
| `narrow` | 768 | The context panel has wrapped below the column. Still the desktop form: it sits above the 640 breakpoint, deliberately. |
| `phone` | 390 | The phone form. The rail is a bottom bar, the aside is stacked in the flow with **no nested scroll region**, and `ItemRow` wraps its title onto its own line. `deviceScaleFactor: 3`, `isMobile`, `hasTouch`. The spec opens the More sheet to reach the five overflow screens, importing the partition from `shell/nav-bar.ts` so it cannot drift. |

The **rule editor at 390 is the one knowingly-exempt screen** (137px over):
its condition rows are a dense grid of selects needing their own design pass.
The exemption is by screen name in `visual/surfaces.spec.ts`, the capture is
still taken, and every other screen at 390 is held to the same bar as desktop.

Captures are **viewport-sized, not `fullPage`**: the shell is
`height: 100dvh; overflow: hidden`, so the document is exactly one viewport on
every project and `fullPage` can only add what is not really there — under the
phone project's `isMobile` emulation it did, reporting 1048px of content for an
844px page.

Themes: `light` and `dark`, seeded into `localStorage` at `hb.theme` before
first paint (the app resolves `light | dark | system` onto
`[data-theme]` — `src/theme/`).

Screen states: the nine screens under `?demo` (deterministic, populated
fixtures — except **Done** and the **Ledger**, which have no demo fixtures
and photograph their "not read yet" holding state; their populated rows are
covered by `DoneScreen.test.tsx`/`LedgerScreen.test.tsx` and reviewed by hand
on a device with real items; **Status** photographs **ten panes** fed by
`src/fixtures/demo-questions.ts` — counted in *panes*, not questions, because
two of its wired questions emit one pane *per subject* the way the race
question does: one `kimi-balance/v1` gauge, five `github-hummingbird/v1`
workflow rows and three `uptime/v1` service rows (#313-#315) make **nine
poller-backed panes**, plus one quiet, device-local `reachability` answer
(#316)), the **capture popover** open
over Now, and **Now's honest empty state** without the flag. What no capture reaches: Triage's **expanded row
editor**, since `?demo` renders the fixture rows (`DemoCapture`) and the editor
only exists over a real `TaskItemDTO` — it is covered by
`screens/TriageScreen.test.tsx` instead, and reviewed by hand on a device with
real captures; and, since #273, **item detail's microtask states** — the two
affordances, the streaming narration, the stamp badge and the decline — for
the same reason one level up: `NowScreen.tsx` branches to `RealFrontier` only
when demo is off, so the item panel is never mounted under `?demo` at all.
Teaching the flag to mount it would mean entangling the demo hero branch with
`RealFrontier`, which that branch exists to prevent, so `visual/surfaces.spec.ts`
is deliberately unchanged here and `components/domain/ItemPanel.test.tsx`
is the cover — which now also covers detail mode's Edit. #274's pinned-decline fallback button ("Switch to `<entry>`")
joins that exclusion under that same cover; the picker it belongs to lives on
**Settings**, which *is* photographed, so the control itself stays in the
matrix even though the decline that offers it does not. Item detail is now
the busiest unphotographed surface in the app, which is worth its own issue
rather than a widening of this one. **Now's
captures in the columns** join that list for the same reason one more level up:
`NowScreen.tsx` branches to `RealFrontier` only when demo is off, so a capture's
card, its `triage` chip, its place under a column's startable actions and the
`TriageRow` editor selecting it opens are never mounted under `?demo`.
`screens/NowScreen.test.tsx` and `screens/TriageScreen.test.tsx` are the
cover; a board mixing both kinds is reviewed by hand on a device with real
captures, which is where a full inbox exists at all. **The Grill takeover**
(#355, ADR-0023) joins that same exclusion for the identical reason:
`TriageRow`'s real "Grill me" button, the question card, the review card and
every turn state (asking, the question, the proposal, a decline) only exist
over a real `TaskItemDTO`, and `?demo` renders `DemoCapture` fixtures with
its own unrelated stub "Grill" button — so this screen never opens a real
takeover under the flag. `screens/GrillTakeover.test.tsx`,
`screens/TriageScreen.test.tsx` and the `shell/useGrillWiring.ts` /
`shell/useGrillTakeoverWiring.ts` hook tests are the cover for every
reachable turn state, and round 2's own tests cover the refused-Confirm
error path (a `needs_re_review` answer leaves the takeover standing and
names itself on the review card). What is still unphotographed is how any
of it *looks*: the not-ready, disconnected and error states still owe
#355's acceptance its own hand pass on a device with a real foggy
capture — not yet performed as of this PR, and worth doing before or
shortly after this lands, since none of it is exercised by `pnpm visual`.
**Now's
frontier columns** join the list for that same reason and are settled up front
rather than discovered mid-slice (ADR-0021 decision 8, #400): because
`NowScreen.tsx` branches to `RealFrontier` only when demo is off, the columns
themselves, the axis switch, the facet-filter panel, the collapsed and
`n more` states and the urgency colours are unphotographed at **every width and
theme**. They take #273's disposition — `screens/NowScreen.test.tsx` plus the
grouping and preference modules' own unit tests are the cover, and the wrap
behaviour, the collapse reflow and the absence of horizontal page overflow are
reviewed by hand at 1440, 1024 and 768 in both themes on a device with real
items, which is where enough columns to wrap exist at all. Entangling `?demo`
with `RealFrontier` to photograph them is rejected there, not merely skipped:
that branch exists to keep the two apart, so widening it would trade a
documented coverage gap for an undocumented behavioural one.

**The capture popover's dictation microphone (#379) is unphotographed, and no
capture will ever contain it.** It renders only where local speech recognition
has been *confirmed* — `available({langs:["en-US"], processLocally: true})`
answering `"available"` (ADR-0022) — which is a browser with the on-device pack
installed. The gate's Chromium has no such pack, so the popover photographs
exactly as it did before this slice, and that is the correct output rather than
a missing state: ADR-0022's `unsupported` and `setup-required` arms both render
nothing at all. The cover is `screens/CaptureBox.dictation.test.tsx` (the seam
mocked wholesale, which is the only way any gate here reaches the listening
state), `speech/local-dictation.test.ts` and
`screens/capture-dictation.test.ts`. **The gate deletes the speech
constructor before the app loads** (`openApp`'s second init script), and that is
not tidiness: headless Chromium 151.0.7922.34 **crashes the renderer** when
`SpeechRecognition.available()` is called, so mounting the capture box killed
the tab and all eight capture-popover cases failed on the click that opens it.
Measured both ways in the same build — headless crashes, `headless: false`
returns `"downloadable"`. Deleting it changes no pixel, since the gate's browser
has no language pack and ADR-0022 renders nothing for either non-`ready` arm. A
renderer crash is not catchable from the page that provokes it, which is why the
defence is in the harness and not a `navigator.webdriver` check in product
code.

The microphone's idle and listening
appearance is reviewed by hand on the desk Chrome, which is the only browser
this repo has measured it on and — until the phone is probed — the only one it
is known to appear in at all.

**Since #420 the columns, the captures among them and #418's stranded-write
alerts ARE photographed** (both of them — the board fixture seeds a failed
triage and a failed act, and `surfaces.spec.ts` asserts the count rather than
the first match, which is what caught the second line arriving) — the twelfth state, `now-columns-*`, and the reason
the count above moved. Not by widening `?demo`, which still never mounts
`RealFrontier` and still means exactly what it meant: by a **second demo
world**, `?demo=board`, which seeds a real `TaskState`
(`src/fixtures/demo-task-state.ts`) and returns `null` for `DemoData`, and a
null `demo` prop is what selects the `RealFrontier` branch. The rejection above
is intact — this is the "decided change with its reasoning written down" that
ADR-0021 decision 8 named as its own condition, and that decision carries the
amendment. The fixture mirrors **production's measured shape and none of its
content** (29 cards, its context/size/energy/source spread, no projects, no
blocked edges), so what the gate photographs is the real awkward board rather
than a tidy one. Still uncovered on this surface and still on the disposition:
everything reached only by interaction — the axis switch, the facet-filter
panel, the collapse reflow and the selected-card slot — since the capture is
one still frame of the default view. The popover is a state rather than a screen — it
renders over whatever is showing (`shell/CapturePopover.tsx`), so no
per-screen capture ever contains it, and the scrim covering the whole window
plus the card fitting inside 768 are only decidable with it open. `?demo` drives
the *real* ranked region through a hand-authored world
(`src/fixtures/demo-questions.ts` — a bound waste question collecting
tomorrow at the address, so what is photographed is an answered, imminent
pane, plus a bound `f1` race question twelve days out, the `distant` state
the race pane holds for most of the year); there is deliberately no demo-only rendering of the region, so the
capture is the shipping component. The empty
states matter on their own: they are what a new device actually shows, and
no fixture screen exercises them. **Rules is populated under `?demo` too**
(#140): `demo-data.ts` carries its own `ruleDetails` / `ruleKindRegistry` /
`ruleBacktestItems`, wired at `App.tsx`'s `screen === "rules"` branch
alongside every other screen's `demo ? … : task.…` split, so its capture is
a deterministic, populated rules screen — condition rows, toggles and a
backtest count — the same as the other five.

There are **no committed golden images and no pixel diff.** The project has
no baseline history, and a pixel gate with nobody to arbitrate it produces
noise rather than findings. The captures are the deliverable — review them
for clipping, overlap, broken wrapping, and sticky/scroll or focus glitches.
What the spec *does* fail on is the machine-decidable subset: horizontal
overflow at any width, unresolved brand tokens, and a theme switch that does
not reach the page.

**Be clear how little the overflow assertion proves.** `App.tsx`'s root is
`overflow: hidden`, so content wider than the shell is *clipped* rather than
extending `documentElement.scrollWidth` — the assertion largely measures the
thing that cannot happen. It caught the rules editor's 137px, which escaped
the clip anyway, so it is not useless; but "every screen passes" means "no
screen scrolls sideways", not "every screen is usable at 390". A row whose
title ellipsised to two characters passed it. Reading the captures is what
found that, and reading the captures is still the gate.

### Not in CI

Deliberately absent from `.github/workflows/client.yml`. `pnpm typecheck`
already rebuilds the wasm core and is that workflow's slow step; a browser
matrix compounds it, and screenshot jobs buy flake before they buy signal
here. Promote it the first time a visual regression actually lands on main.

Requires a one-time `pnpm exec playwright install chromium` per machine.

### Brand-token bindings

| Binding | Where |
| --- | --- |
| Design system source | The "Hummingbird Design System" project on claude.ai/design |
| Repo-local mirror | `.claude/skills/hummingbird-design/` (invoke `/hummingbird-design`) |
| Consumed copy | `client/web/src/design/tokens/` — `fonts.css` is swapped to self-hosted `@font-face`, because the production CSP allows no Google Fonts |
| Tailwind mapping | `client/web/src/styles.css`; dark mode on `[data-theme="dark"]` |

Layout constants the matrix above is derived from live in
`design/tokens/spacing.css`: `--rail-width: 236px`, `--panel-width: 320px`,
`--content-max: 880px`, `--touch-min: 44px`.

The breakpoint is **not** among them, and must not be added there:
`src/design/tokens/` is a copied mirror of the design project and a re-pull
silently deletes anything local. It is an app constant
(`src/shell/breakpoints.ts`). A breakpoint scale taken upstream to the design
project is a separate piece of work.

The phone form amends one statement of the design README directly: *"The rail
and the panel are fixed; only the centre column scrolls."* Below 640 the rail
is a bottom bar and the panel is stacked in the flow. Only the centre column
still scrolls — that half holds.

The visual spec asserts these tokens **resolve**, never that they equal a
particular hex or pixel value — the design system owns the values, and a
re-pull must be free to change them. When it changes: re-pull the mirror
first, re-copy tokens into `client/web/src/design/`, then re-run the gate
across every screen, not only the ones a diff touched.

---

## Surface: the authority server

No visual surface. `server/` is an API — Worker + Durable Object — with no
rendered output. Gated by `.github/workflows/server-test.yml` — the shared
recipe that runs `server/scripts/smoke.sh`, called by
`.github/workflows/server.yml` on pull requests and by
`.github/workflows/deploy-server.yml` on `main` — not by anything in this
file.

## Planned, not built

The design system carries UI kits for **native Android**, **Wear OS** and
**iOS** (`.claude/skills/hummingbird-design/ui_kits/`). None has code in this
repo, so none has a gate here. Add a surface section when one gets a code
root — an emulator/simulator matrix, per the `/wrapup` reference.

---

## What component tests cover, and what this does not

The visual gate answers "does it look right". It does not answer "is it
wired" — three of the four PRs in the S10–S13 batch shipped UI state with no
reader, all of which rendered fine. That gap is covered by the component
tests (`client/web/src/**/*.test.tsx`, jsdom, run by `pnpm test`) and by
`client/web/src/test/component.tsx`, which explains the failure mode. Neither
gate substitutes for the other.
