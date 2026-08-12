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
| **Screens** | `screens/*.tsx` — Now, Triage, Routes, Alerts, Rules, Done, Ledger, Settings |
| **Now's aside** | `screens/questions/RankedRegion.tsx` — ADR-0015's ranked standing-question region (#245), plus each question's own expanded pane (`screens/waste-pane/`, `screens/weekend-pane/` #122, `screens/vacation-pane/` #121, `screens/race-pane/` #119 — the first question emitting one pane *per subject*, so the aside's height varies with the `race-series` binding). It replaced the calendar context tile, so the aside now *grows* with the number of questions: `screens/layout.tsx`'s `Aside` caps at `100dvh` and scrolls itself, which is a change every screen with an aside inherits (Now, Settings, Alerts, Routes). |
| **Shell** | `shell/Header.tsx`, `shell/NavRail.tsx`, `shell/CapturePopover.tsx` (the capture box, over any screen), `shell/UpdateBanner.tsx` (the "new version — reload" strip, under the header), `screens/layout.tsx` |
| **Components** | `components/{core,forms,domain,feedback}/` — the 16-component library |
| **Toolset** | Playwright (`client/web/playwright.config.ts`, `client/web/visual/`) |
| **Command** | `cd client/web && pnpm visual` |
| **Captures** | `client/web/visual/.captures/` (gitignored) |

### Matrix

Three widths × two themes × ten screen states, per run.

| Project | Width | What it proves |
| --- | --- | --- |
| `wide` | 1440 | Rail, content column and context panel side by side — the design target. |
| `boundary` | 1024 | The wrap point. 236px rail + 380px minimum column + 320px panel plus gaps lands within a few pixels of this, so it is where `screens/layout.tsx`'s `TwoColumn` decides. A layout regression shows here first. |
| `narrow` | 768 | The context panel has wrapped below the column. |

Themes: `light` and `dark`, seeded into `localStorage` at `hb.theme` before
first paint (the app resolves `light | dark | system` onto
`[data-theme]` — `src/theme/`).

Screen states: the eight screens under `?demo` (deterministic, populated
fixtures — except **Done** and the **Ledger**, which have no demo fixtures
and photograph their "not read yet" holding state; their populated rows are
covered by `DoneScreen.test.tsx`/`LedgerScreen.test.tsx` and reviewed by hand
on a device with real items), the **capture popover** open over Now, and
**Now's honest empty state** without the flag. What no capture reaches: Triage's **expanded row
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
is the cover. Item detail is now the busiest unphotographed surface in the
app, which is worth its own issue rather than a widening of this one. The popover is a state rather than a screen — it
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
