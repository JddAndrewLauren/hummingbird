# Surface registry

The registry of this repo's visual surfaces: what renders, where its code
lives, which toolset gates it, and at what matrix. `/wrapup`'s visual
verification phase reads this file, maps the session diff against it, and
runs the listed gate for **each affected surface, scoped to the affected
screens**.

A **brand-token change affects every surface**, not just the screens a diff
touched — see "Brand-token bindings" below.

---

## Surface: desktop web

The only built surface. `hb.twinion.net` (ADR-0006), a PWA offline shell
served from Cloudflare Workers static assets. Not deployed yet — #95's H3
human gate.

| | |
| --- | --- |
| **Code root** | `client/web/src/` |
| **Screens** | `screens/*.tsx` — Now, Triage, Routes, Alerts, Rules, Done, Ledger, Status, Settings |
| **Now's aside** | `screens/questions/RankedRegion.tsx` — ADR-0015's ranked standing-question region (#245), and the landmark is named **"Standing questions"** for it (#401, ADR-0021 decision 6: it read `Context` long after the context tile stopped being there, and the word was needed for the centre column's grouping axis) — plus each question's own expanded pane (`screens/waste-pane/`, `screens/weekend-pane/` #122, `screens/vacation-pane/` #121, `screens/race-pane/` #119 — the first question emitting one pane *per subject*, so the aside's height varies with the `race-series` binding). It replaced the calendar context tile, so the aside now *grows* with the number of questions: `screens/layout.tsx`'s `Aside` caps at `100dvh` and scrolls itself, which is a change every screen with an aside inherits (Now, Settings, Alerts, Routes). |
| **Now's centre column** | `screens/NowScreen.tsx`'s `RealFrontier` — the frontier in **wrapping columns** (`screens/FrontierColumns.tsx`, grouped by a switchable axis — Context, Project, Size or Energy — over the pure `screens/frontier-columns.ts`; #402, ADR-0021), then Blocked, then `screens/NowTriageSection.tsx`: the triage inbox brought under the promoted items, collapsible (persisted device-locally by `screens/triage-collapse.ts`) and capping its own list at `60dvh`. That cap is the **only independent scroll container in the centre column** — everywhere else the shell's one container scrolls the page — so a long inbox is the state to check it in, and it is a **live constraint on the columns**: they wrap onto more lines instead of scrolling sideways, and no column overflows on its own (each caps at six cards with an `n more` toggle). Colour the card itself introduces encodes urgency and nothing else (the stage chip and priority label are `ItemRow`'s, and stage is one of the three things the design system lets colour mean), and the urgency is stated in words too — in text colours, not the swatch's, which is a contrast requirement rather than a taste one. Since #403 each column header collapses **in place** (shrink-to-fit, so neighbours reflow rather than leaving a hole) and a Filter button opens a facet panel (context/size/energy/urgency, OR within a facet and AND across) with an `n of m shown` readout; the axis and the collapsed set persist device-locally via `screens/frontier-prefs.ts`, the filter selection deliberately does not, and changing the axis clears the collapsed set. Since #404 **selecting a card is not a takeover**: the existing `ItemDetailPanel` expands *above* the columns, which stay mounted and visible under it, with the source card marked (`aria-current` plus an accent fill) and the panel scrolled into view — `RealFrontier` used to return the panel *instead of* the frontier. The triage section is still withheld while the panel is open, which is the one thing #404 left alone: `TriageRow`'s expanded editor and `ItemDetailPanel` are both editors, and S13/#111's "two editors are never open at once" is unrelated to keeping the alternatives in view. |
| **Status** | `screens/StatusScreen.tsx` (#311, ADR-0017) — the same `RankedRegion` as Now's aside, instantiated a second time (`surface="status"`) as a single-column screen rather than an aside; three of its four infra questions are now wired to real pollers (`screens/kimi-pane/` #313, `screens/github-pane/` #314, `screens/uptime-pane/` #315), and `screens/reachability-pane/` remains a placeholder pending #316, so today it renders one gap pane plus whatever the three wired questions currently answer. |
| **Shell** | `shell/Header.tsx`, `shell/NavRail.tsx`, `shell/CapturePopover.tsx` (the capture box, over any screen), `shell/UpdateBanner.tsx` (the "new version — reload" strip, under the header), `screens/layout.tsx` |
| **Components** | `components/{core,forms,domain,feedback}/` — the 16-component library |
| **Toolset** | Playwright (`client/web/playwright.config.ts`, `client/web/visual/`) |
| **Command** | `cd client/web && pnpm visual` |
| **Captures** | `client/web/visual/.captures/` (gitignored) |

### Matrix

Three widths × two themes × eleven screen states, per run.

| Project | Width | What it proves |
| --- | --- | --- |
| `wide` | 1440 | Rail, content column and context panel side by side — the design target. |
| `boundary` | 1024 | The wrap point. 236px rail + 380px minimum column + 320px panel plus gaps lands within a few pixels of this, so it is where `screens/layout.tsx`'s `TwoColumn` decides. A layout regression shows here first. |
| `narrow` | 768 | The context panel has wrapped below the column. |

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
poller-backed panes**, plus the one remaining gap pane, `reachability`,
pending #316), the **capture popover** open
over Now, and **Now's honest empty state** without the flag. What no capture reaches: Triage's **expanded row
editor**, since `?demo` renders the fixture rows (`DemoCapture`) and the editor
only exists over a real `TaskItemDTO` — it is covered by
`screens/TriageScreen.test.tsx` instead, and reviewed by hand on a device with
real captures; and, since #273, **item detail's microtask states** — the two
affordances, the streaming narration, the stamp badge and the decline — for
the same reason one level up: `NowScreen.tsx` branches to `RealFrontier` only
when demo is off, so `ItemDetailPanel` is never mounted under `?demo` at all.
Teaching the flag to mount it would mean entangling the demo hero branch with
`RealFrontier`, which that branch exists to prevent, so `visual/surfaces.spec.ts`
is deliberately unchanged here and `components/domain/ItemDetailPanel.test.tsx`
is the cover. #274's pinned-decline fallback button ("Switch to `<entry>`")
joins that exclusion under that same cover; the picker it belongs to lives on
**Settings**, which *is* photographed, so the control itself stays in the
matrix even though the decline that offers it does not. Item detail is now
the busiest unphotographed surface in the app, which is worth its own issue
rather than a widening of this one. **Now's
triage section** joins that list for the same reason one more level up:
`NowScreen.tsx` branches to `RealFrontier` only when demo is off, so the
section — its header toggle, its collapsed state, its capped scroll container
and the `TriageRow` editors inside it — is never mounted under `?demo`.
`screens/NowScreen.test.tsx` and `screens/triage-collapse.test.ts` are the
cover; the scroll cap and the frontier-above-it are reviewed by hand on a
device with real captures, which is where a full inbox exists at all. **Now's
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
documented coverage gap for an undocumented behavioural one. The popover is a state rather than a screen — it
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
`--content-max: 880px`.

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
