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

The only surface with a *visual* gate, in **two forms**: desktop, and — since the mobile pass
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
| **Screens** | `screens/*.tsx` — Now, Triage, Projects, Alerts, Rules, Done, Ledger, Status, Settings |
| **Now's aside** | `screens/questions/RankedRegion.tsx` — ADR-0015's ranked standing-question region (#245), and the landmark is named **"Standing questions"** for it (#401, ADR-0021 decision 6: it read `Context` long after the context tile stopped being there, and the word was needed for the centre column's grouping axis) — plus each question's own expanded pane (`screens/waste-pane/`, `screens/weekend-pane/` #122, `screens/vacation-pane/` #121, `screens/race-pane/` #119 — the first question emitting one pane *per subject*, so the aside's height varies with the `race-series` binding — and `screens/homework-pane/` #675, which sorts **first** and is the first question keyed on the operator's own items rather than an outside source: its body is the winning `@homework` item, the preparation notes written on it, and the other open ones listed beneath, read-only). **The homework pane adds no screen state to the matrix below and is not photographed on its own**: it has no setup prompt to reach (nothing binds it) and no second surface, so it is covered wherever this aside already is: the `now-*` and `now-columns-*` captures at `wide` and `boundary`, which are the two widths that put the aside beside the column rather than below the fold (eight captures — the same reach every other Now pane has, not a gap this one introduces). Its real body shows there only because the board seed carries two `@homework` items on purpose (`demo-task-state.ts`'s departure 5); without them all eight would photograph its dormant "No open homework" line and the body would be covered nowhere. The `now-empty-*` captures (no `?demo` flag) do show that dormant line, which is the empty world's correct answer. **The pane's standing session link is covered by those same eight captures and adds none of its own**: it is a "Join the session" `Button` appended to the pane in every arm it has, so it is photographed wherever the pane already is, and the 320px aside is exactly the width that has ellipsised a label here before. The URL behind it is a *binding* (`homework-link`), which is what makes it photographable at all — the board seed carries a **fictional** one (`demo-task-state.ts`'s `boundHomeworkLinkBinding`), and it must stay fictional: the real value carries a meeting passcode and this repo is public. The `now-empty-*` captures have no bindings, so they correctly show the dormant card with no button. **`screens/scps-pane/` (#693, ADR-0032) is covered only in its collapsed, `bound-but-unacquired` "Waiting for the first calendar sync" row — its EXPANDED body (kind/time/location/notes, then the quest, then any further events) is photographed at NO width, and that is a real gap, not the homework precedent restated.** Homework's body is reachable from the demo fixture because its subject is `TaskState`, which `demo-task-state.ts` owns and injects at the store boundary; SCPS's subject is the calendar arm, which is threaded live only (`App.tsx`'s `calendar` from `useCalendarWiring`) — `demo-calendar.ts`'s own header states why a seeded `connected: true` cannot be handed to that hook: it runs real effects (a 15-minute poll timer, token rotation) against a worker holding no real token, so a fake connected state would either silently do nothing or misbehave, never render the honest fiction `demo-task-state.ts` achieves for items. **This is not new with SCPS**: `screens/weekend-pane/` (#122) and `screens/vacation-pane/` (#121) share the identical gap and always have — neither pane's real expanded body has ever been photographed by this gate either, both always rendering their `unbound` "not set up" card in every capture that shows the aside. Closing it needs a demo-mode calendar override with the same shape `demo-task-state.ts` gives `TaskState` — one that fabricates `calendarReads`/`calendarConnected` without waking `useCalendarWiring`'s live effects — which is real, scoped plumbing work belonging to whichever slice next needs a calendar-arm pane's real body photographed, not a one-line fix bundled into this one. Until then, the specific risk this gap could hide — free-text `notes`/`location` overflowing or ellipsising in the 320px aside, `ItemRow`'s own documented failure mode — is covered instead by `ScpsPaneExpanded.test.tsx`'s component tests, which assert the real strings render, though jsdom cannot catch a pixel-level overflow the way this gate's PNG would; a human visual pass on a real device is the operator's own check until the demo-calendar override exists. The region replaced the calendar context tile, so the aside now *grows* with the number of questions: `screens/layout.tsx`'s `Aside` caps at `100dvh` and scrolls itself, which is a change every screen with an aside inherits (Now, Settings, Alerts, Projects). **On the phone form all three of those properties are undone** (`.hb-aside` in `shell/responsive.css`): sticky + `100dvh` + `overflow-y: auto` on a full-width panel below the column would make a nested scroll region, and the reachability problem they solve does not exist once the panel is stacked in the flow — the page scrolls its whole height. The panel now also *reads* as a region rather than as more of the same column: `.hb-aside` carries the design system's `--surface-quiet` wash (the 4% "where a surface needs to recede" tint, and explicitly **not** a card fill — the panel holds cards of its own, and painting it card-coloured would flatten the two elevations into one), plus `--radius-card` and `--space-5` of padding. **The whole aside collapses, and the control that does it is the shell's, not the panel's**: one `IconButton` in `shell/Header.tsx`, between Refresh and New, showing a rotated `chevron-down` while the panel is open and `help-circle` once it is shut — the same slot in both states, because a toggle that lives inside the thing it hides moves when you press it and then has to be found twice. Shut, the aside **unmounts entirely** rather than shrinking to a strip (an `aside` labelled "Standing questions" holding nothing is a landmark that lies to a screen reader), and the centre column takes the freed width — `.hb-two-column` wraps, so the column simply grows. The panel therefore carries no heading and no control of its own; the landmark's `aria-label` is where the name lives for anyone who cannot see it. The state is `App.tsx`'s `asideCollapsed`, held beside `railCollapsed` and persisted device-locally by `screens/questions/aside-prefs.ts` (open is the default and is stored as key *absence*, the `frontier-prefs.ts` idiom) — **not** `NowScreen`'s, which is controlled here and owns neither the toggle nor the persistence, because the button belongs to the shell and the panel belongs to Now. **One state this region could not be in before #715**: every Now question switched off, which renders a compact `EmptyState` ("Nothing is being asked") naming Settings rather than leaving the panel blank — the quietly-empty answer ADR-0015 rules out, and one the reader caused themselves. It is not photographed: reaching it means switching off all six from Settings inside one capture pass, and the state is asserted instead in `NowScreen.test.tsx`. |
| **The frontier board** (Now's centre column, and a project dossier's) | `screens/FrontierBoard.tsx`'s `FrontierBoard` — extracted from `NowScreen.tsx` when the project dossier's centre column became this same board filtered to one project's items, so what follows is true of BOTH surfaces except where it names Now: the frontier in **columns packed into lanes** (`screens/FrontierColumns.tsx`, grouped by a switchable axis — Context, Project, Size or Energy — over the pure `screens/frontier-columns.ts`; #402, ADR-0021), then Blocked. The **triage process (CONTEXT.md — Triage and Grilling together) is cards in those same columns** rather than a section of their own: `screens/triage-process-order.ts`'s `triageProcessQueue` (#357) combines `TaskState.triageInbox` and `TaskState.grillingItems` — drafts first, then Grilling, then captured Triage — and that ONE ordered read is appended to the ordered frontier before grouping, so each item lands in whichever column the live axis puts it in (the no-value one until something sets that field) and sits **under** that column's startable actions, marked by its own `StageBadge` (`triage` or `grilling`) — the same stage vocabulary the Triage screen's rows use, and not a fourth meaning for colour. This is also Now's own half of the "triage process" queue: the Triage screen renders the identical function over the identical two reads, so the header's exact `captured · grilling` counts and the combined order never drift between the two surfaces. There is now **no independent scroll container in the centre column at all** — the shell's one container scrolls the page — which the columns already assumed: they stack into as many vertical lanes as the measured width affords instead of scrolling sideways, and no column overflows on its own (each caps at six cards with an `n more` toggle). The lane count and the packing are `screens/frontier-lanes.ts` (`laneCountFor`/`packLanes`) — greedy, shortest-lane-first over each column's *rendered row count*, so short columns stack under one another rather than each claiming a full track beside a tall one. It is TS rather than Rust because it consumes a measured pixel width (ADR-0025; that module's header carries the argument, including why CSS multi-column and grid masonry could not deliver it), and an unmeasured board — the first paint, and every jsdom test — falls back to one column per lane, which is the pre-lanes layout. Colour the card itself introduces encodes urgency and nothing else (the stage chip, the priority label and the **size and energy chips** are `ItemRow`'s, and stage is one of the three things the design system lets colour mean; since #446 size and energy are coloured glyphs drawn from their own ramp, which is the cost ADR-0024 decision 2 accepts against ADR-0021 — an amber *mark* on a card can mean due-soon or normal-size, and only the card's own colour still means urgency alone), and the urgency is stated in words too — for the three coloured bands only, since `calm` no longer prints a word any more than it takes a swatch (ADR-0021 decision 2: the default is not a claim) — which made every entry on the card's meta line conditional, so the line itself (`FrontierColumns.tsx`'s `CardMeta`, and `NowScreen.kt`'s guarded meta `Row` on the phone) draws only when it has something on it rather than stranding an empty row's gap under the title — and in text colours, not the swatch's, which is a contrast requirement rather than a taste one. Since #403 each column header collapses **in place** (it keeps its lane's width now rather than shrinking to fit — the packing is what buys the space back, since a collapsed column weighs one row and the lanes rebalance around it) and a Filter button opens a facet panel (context/size/energy/urgency, OR within a facet and AND across) with an `n of m shown` readout; the axis and the collapsed set persist device-locally via `screens/frontier-prefs.ts`, the filter selection deliberately does not, and changing the axis clears the collapsed set. Since #404 **selecting a card is not a takeover**: the item panel expands *above* the columns, which stay mounted and visible under it, with the source card marked (`aria-current` plus an accent fill) and the panel scrolled into view — the board used to return the panel *instead of* the frontier. Selecting a **capture** fills that same slot with `TriageRow` forced open, so S13/#111's "two editors are never open at once" holds by construction — one slot, one editor — and the captures' cards stay on the board whichever kind is open. Both are now the **same component** (`components/domain/ItemPanel.tsx`) in its two modes: `"triage"` stands the fields open and ends in the promotion to Ready plus Grill me (#360: Ready is triage's only destination — an item reaches Grilling exactly one other way, a `fog_remains` Grill verdict), `"detail"` reads as a record until **Edit** reveals the identical fields and saves them through `Core::triage` with `destination: null` (#122's stage-agnostic edit) — before that fold, a minted action's own fields were reachable nowhere in the app. Since #359 `"detail"` also offers **Grill me**/**Resume grill**, gated by `item-actions.ts`'s `canGrill` (now true for Ready and In Progress, not Triage alone) — pressing it replaces this whole centre column with `GrillTakeover`, the identical component Triage's own row opens, while the standing-questions aside beside it (a sibling of this column, not nested inside it) stays mounted throughout; Back closes the takeover and restores focus to the button by DOM id (`FrontierBoard.tsx`'s `nowGrillMeButtonId`, minted only on Now — the project board is threaded no `grill` at all, so the takeover is unreachable from a project), the same "look it up by id, never a held ref across the unmount" contract `TriageRow.tsx`'s `grillMeButtonId` uses. Clicking the open card again closes it, and Escape closes the panel from anywhere (`shell/capture-hotkey.ts`'s `closesItemDetail`, which yields to the capture popover). Since #418 a **failed write is stated above the columns, naming the item** (`screens/write-failure.ts`), for the case the slot made reachable: the editor that would otherwise wear the failure is unmounted the moment the reader closes the panel. There are **two such lines, not one** — a failed triage and a failed act are separate results in the store (`lastTriage`, `lastAct`), so a shared slot would let one failure hide the other. Each is suppressed while the editor that owns it is what the slot holds — for a triage, either editor, since detail mode says its own failures once it can edit — so no result is ever stated twice; the act line is *not* suppressed for an open capture, whose `TriageRow` renders no act failure though its checkmark issues one. |
| **Status** | `screens/StatusScreen.tsx` (#311, ADR-0017) — a thin wrapper over `screens/status-board/StatusBoard.tsx`, the design handoff's **board of expanding tiles**: the same `rankPanes(inputs, "status")` panes the surface has always had, drawn as two labelled grids (`infra`, `capture & context sources`) of compact tiles, where clicking a tile expands *it* across two grid columns and shows the pane's own body in place. Single selection — one open tile, persisted device-locally by `status-board/status-prefs.ts` (`hb.status.expanded`, absence = nothing open). **Status no longer instantiates `RankedRegion`**, which stays Now's aside alone: a board needs neither the captured sample (tile position is a function of *identity* — group, then declared question order, then subject — so a band change cannot move a tile at all, which is strictly stronger than freezing a sample) nor `questions/collapse.ts` (there is no per-pane collapse to override; a problem announces itself by its tile's treatment rather than by opening itself). The one datum no decision produces is which grid and glyph a pane gets, a literal table in `status-board/tile-vocabulary.ts` with a fallback on both axes and a drift test over the registry. Shipped `.status`-suffixed collapse keys are deliberately left unmigrated — view preferences, where absence is the default. Three questions read real pollers (`screens/kimi-pane/` #313, `screens/github-pane/` #314, `screens/uptime-pane/` #315), and `screens/reachability-pane/` #316 answers from this device's persisted authority-sync history, with no poller or source of its own; each pane's expanded body is reused through the new `XPaneBody` exports beside each `XPaneExpanded` — the same components Now renders, minus the card the tile already is and minus their own headline, which the tile's header carries (a gap keeps its reason but loses its heading, so an unpolled board does not say every sentence twice). Reachability has nothing left under its headline, so its tile is the one drawn **without** a toggle: nine `aria-expanded` on a ten-tile board, which is what `surfaces.spec.ts` now counts. **#715 adds one board state that could not exist before it**: every Status question switched off, which replaces the two grids with the same compact `EmptyState` Now's aside uses and **keeps the sync strip**, the one reading on this board that no toggle governs. Not photographed, for the reason the aside's is not; asserted in `StatusBoard.test.tsx`. |
| **Settings' standing questions** | `screens/SettingsScreen.tsx`'s `#standing-questions` section (#714, ADR-0034 decision 4) — since this slice a list of **questions**, each with the binding rows that answer it indented beneath a hairline, rather than the flat list of `BindingKey` rows it was. The spine is the core's roster (`decisions::questions`, reached through `screens/questions/roster.ts`), so the ten headings, their order, their operator-facing names and the question→binding relation are all decided once and drawn twice — the calendar picker's locked-row hint reads its question's name from the same list rather than the sentence that used to be hand-written into it. **A question with no binding still renders**, with `Nothing to set` under it: ADR-0034 makes this section the one place a question can be seen when its own pane is quiet, so an omission here would hide a question rather than tidy a list. Live `settings` rows no question claims — the keys this build cannot write, which `Core::bindings` returns on purpose — keep a final **Other settings rows** group and stay read-only. The section is **not** device-token-gated (the calendar section above it is, #585) and is gated on `status === "ready"`, which the empty-world `settings-*` captures photograph as the honest note rather than as ten empty groups. **One new screen state**, `settings-empty-*` (four widths × two themes), taken inside the existing empty-states test: Settings on a device that has bound nothing, which the `?demo` world cannot reach because it hand-authors its bindings. That frame is the one worth photographing now that this section is where an unbound question is discovered — and it is the frame that caught this slice's own copy bug, a question whose declared key had no row reading as a question with nothing to set (the two are separated: `No settings row for scps-quest yet.` versus `Nothing to set`, the former reachable only in the demo world, since `Core::bindings` returns every known key). The seeded `settings-*` captures cover the reshaped section at the same four widths, and `visual/surfaces.spec.ts` counts the section's `h3`s in both worlds so a question dropping out of the roster fails the gate rather than passing as a screen that merely looks similar. **The roster renamed one question.** `reachability` was *This device*, which is also the title of the Settings section immediately below the roster — one screen must not say the same phrase twice about two different things, and this section is where the collision appeared, because before it the question's name was drawn only on the Status board. It is now *Is this device reachable*, matching the interrogative register the Now questions use. **That renames a Status tile too**, since the reachability pane's headline carries no subject of its own and `tile-copy.ts` therefore uses the question's label as the tile's bold line — checked at every width, including 390, where it sets on one line without wrapping or ellipsising. **#715 made each row a disclosure with a toggle in it** (ADR-0034 decisions 1-3): the heading is now a full-width `ControlButton` *inside* the same `h3` (`FrontierColumns.tsx`'s own collapse idiom, by way of `sectionToggleStyle`, so the heading keeps the question's name as its accessible name and the `h3` count is unchanged), and opening a row reveals an **Asked** `Switch` above that question's binding rows. Rows are **collapsed by default** and the open set is device-local (`screens/question-prefs.ts`, the injectable-`storage` idiom — never a `settings` row, which is the sharpest instance of `bindings.rs`'s rule, since the row it belongs to is the one whose *toggle* does sync). Two state badges stay readable with the row shut — `off` and the `queued` overlay badge — because an off question is discoverable nowhere else; at 390 the header row wraps rather than squeezing the question's name. **Three new screen states**, all inside the existing empty-states test and all driven through the REAL core rather than a fixture (the `?demo` world overrides `TaskState`, so a click there would change nothing): `settings-empty-*` (also the collapsed roster — ten rows, no toggles), `settings-question-expanded-*` and `settings-question-off-*`, four widths × two themes each. The seeded `settings-*` captures open all ten rows first, which is the frame that pass photographed before the rows became disclosures. **#707 adds two controls that render regardless of `status`**, not inside this section's own `#standing-questions` gate: "Download diagnostics" and "Clear diagnostics", in the aside's "core" card right after **Local core** (`worker/diagnostics-journal.ts`'s SharedWorker journal, `worker/ports.ts`'s `DiagnosticsPortHandler` making the request servable even from a core that failed to initialize) — the one place on this screen deliberately NOT gated on `status === "ready"`, since an unreachable core is exactly when the journal is needed. `visual/surfaces.spec.ts` asserts both are `toBeVisible()` in the board-world `settings` capture and in the real-core `settings-empty-*` capture — a real Playwright check, not a screenshot a human might or might not scroll to, since both captures are viewport-sized (`fullPage: false`) and this card sits below the fold at every width. Not separately photographed for the `loading`/`error` states: reproducing a wasm load failure inside this end-to-end browser spec is out of scope here and is covered instead by `SettingsScreen.test.tsx`'s own "reachable regardless of core status" block, which mounts the screen directly at `status: "loading"` and `status: "error"` and asserts the controls render and are clickable in both. |
| **Shell** | `shell/Header.tsx` (title, sync pill, Search, Refresh, **the standing-questions toggle** — supplied only on Now, on the same "the affordance appears where it would work" rule as the other two — and New), `shell/NavRail.tsx` (desktop) / `shell/NavBar.tsx` (phone — four screens plus a More sheet, partitioned by `shell/nav-bar.ts`; `App.tsx` mounts exactly one, since two navigation landmarks break the spec's strict-mode `getByRole("navigation")`), `shell/ShellMeta.tsx` (the core-state and build-version lines, in the rail's footer and at the foot of the More sheet — on a phone that sheet and Settings are the only two places the build version is reachable), `shell/CapturePopover.tsx` (the capture box, over any screen), `shell/RecallOverlay.tsx` (**Recall** — #478/#479/#480/#481 — the search overlay, over any screen, reachable from four triggers: the header's Search button, the `/` hotkey, the rail's magnifier and the phone More sheet's entry), `shell/UpdateBanner.tsx` (the "new version — reload" strip, under the header), `screens/layout.tsx`, `shell/responsive.css` |
| **Components** | `components/{core,forms,domain,feedback}/` — the 20-component library. Counted as *components a screen calls*: the eight size/energy glyph primitives in `core/custom-glyphs.tsx` (#446, ADR-0024) are not among them, since they exist only to fill `Icon`'s `ICON_MAP` and no caller names one directly. The twentieth is `forms/Combobox.tsx`, the open-vocabulary counterpart to `Select` — a `<select>`'s values *are* its vocabulary, so the one field whose vocabulary the schema leaves open (`items.context`) could not be one; it wraps `Input` and, since #641 made capture's Context sticky and the native `<datalist>`'s unsuppressable substring filter hid the rest of the vocabulary behind it, **a listbox of our own** — the popup is therefore a real visual state (see the matrix's capture-popover rows), where the native one was browser chrome no screenshot could see. `screens/field-vocabulary.ts` carries the vocabulary decision and the component's own header carries the listbox one. |
| **Toolset** | Playwright (`client/web/playwright.config.ts`, `client/web/visual/`) |
| **Command** | `cd client/web && pnpm visual` |
| **Captures** | `client/web/visual/.captures/` (gitignored) |

### Matrix

Four widths × two themes × twenty-three screen states, per run.

| Project | Width | What it proves |
| --- | --- | --- |
| `wide` | 1440 | Rail, content column and context panel side by side — the design target. |
| `boundary` | 1024 | The wrap point. 236px rail + 380px minimum column + 320px panel plus gaps lands within a few pixels of this, so it is where `screens/layout.tsx`'s `TwoColumn` decides. A layout regression shows here first. |
| `narrow` | 768 | The context panel has wrapped below the column. Still the desktop form: it sits above the 640 breakpoint, deliberately. |
| `phone` | 390 | The phone form. The rail is a bottom bar, the aside is stacked in the flow with **no nested scroll region**, and `ItemRow` wraps its title onto its own line. `deviceScaleFactor: 3`, `isMobile`, `hasTouch`. The spec opens the More sheet to reach the five overflow screens, importing the partition from `shell/nav-bar.ts` so it cannot drift. |

The **rule editor OPEN at 390 is the one knowingly-exempt state** (137px
over): its condition rows are a dense grid of selects needing their own
design pass. The exemption in `visual/surfaces.spec.ts` applies only after
the editor opens — the Rules screen's own LIST state is checked at 390 like
every other screen, unconditionally, before the editor opens for the
capture — so this is one state exempt, not the whole screen.

**Rules is photographed twice**, `rules-list-*` and `rules-*`, and the pair is
not redundant. The editor the second capture opens on rule 1 — condition rows
plus an expanded backtest — is tall enough to push the seed's fourth rule off
the bottom of every viewport, and that fourth rule
(`fixtures/demo-data.ts`'s `rule-unranked-severity`) exists only to photograph
#374's wrapping badge row. For a run it was in no capture at all: at 1440 only
the badge's top edge survived, and at 1024 and below the whole card was below
the fold. So the list state is shot first, with the badge scrolled into view.

Captures are **viewport-sized, not `fullPage`**: the shell is
`height: 100dvh; overflow: hidden`, so the document is exactly one viewport on
every project and `fullPage` can only add what is not really there — under the
phone project's `isMobile` emulation it did, reporting 1048px of content for an
844px page.

Themes: `light` and `dark`, seeded into `localStorage` at `hb.theme` before
first paint (the app resolves `light | dark | system` onto
`[data-theme]` — `src/theme/`).

Screen states: the nine screens under the default `?demo` — the **board**
world (#420, #455), a seeded `TaskState` that takes the screens' real render
path with fictional data in it, deterministic and populated on **eight** of
the nine, including **Done** and the **Ledger** (#452 grew the seed to cover
them; before that only the frontier and the triage inbox were seeded, and
`?demo` meant the design kit besides) and **Projects** (#624's departure 4,
which is why that screen replacing Routes moved this count). **Alerts is the
one exception**: it does not read `TaskState` at all, so it photographs the
same honest empty state an unseeded device would — see "Alerts lost its only
capture at #455's flip" below. **Status** photographs **ten
panes** fed by the seed's own `bindings`/`paneReads`
(`src/fixtures/demo-pane-reads.ts`) — the kit world's own `demo-questions.ts`
duplicated this before #452 folded its content into the seed, and #455
deleted the now-redundant module entirely — counted in *panes*, not
questions, because two of its wired questions emit one pane *per subject* the
way the race question does: one `kimi-balance/v1` gauge, five
`github-hummingbird/v1` workflow rows and three `uptime/v1` service rows
(#313-#315) make **nine poller-backed panes**, plus one quiet, device-local
`reachability` answer (#316)). Since the board landed those ten photograph
as **two labelled tile grids** rather than a stack of rows, and the board
adds one state of its own — `status-expanded-*`, a tile opened across two
grid columns with a pane's body inside it, which is the shape the
all-compact per-screen capture cannot show and the widest thing the surface
draws. Also photographed: the **capture popover** open
over Now, and **Now's honest empty state** without the flag. What no capture reaches: Triage's **expanded row
editor**. `TriageScreen` has one render path now (#456 deleted its `demo`
prop along with the kit-only fixture card list): the rows are always real
`TaskItemDTO`s with a real editor wired, but nothing in
`visual/surfaces.spec.ts`'s board Triage capture opens one — it asserts only
the header count. It is covered by
`screens/TriageScreen.test.tsx` instead, and reviewed by hand on a device with
real captures; and, since #273, **item detail's microtask states** — the two
affordances, the streaming narration, the stamp badge and the decline — for a
related reason one level up: the board world's own Now capture never selects
a card to expand `ItemPanel` at all (it asserts the `@computer` heading and
the two stranded-write alerts, nothing more), so nothing in this file reaches
the panel, real microtask wiring or not. `visual/surfaces.spec.ts` is
deliberately unchanged here and `components/domain/ItemPanel.test.tsx`
is the cover — which now also covers detail mode's Edit. #274's pinned-decline fallback button ("Switch to `<entry>`")
joins that exclusion under that same cover; the picker it belongs to lives on
**Settings**, which *is* photographed, so the control itself stays in the
matrix even though the decline that offers it does not. **Item detail's core
render is no longer unphotographed** (#481): Recall's own board-world
captures mount the identical `components/domain/ItemPanel.tsx` in `"detail"`
mode — a live result's Edit form once pressed
(`recall-live-expanded-*`), and a Done/archived result's read-only record
with no Edit at all (`recall-readonly-expanded-*`) — so those two states are
in the matrix now, reached through Recall rather than through the
frontier board. (The live row's own pre-Edit render is deliberately not a
third capture: it shows the same read-only record the archived capture
already photographs, plus an Edit button — the spec's own comment on the
live test says why it presses Edit instead.) What stays unphotographed is
everything above that Recall
never wires: `showSteps={false}` and no `microtask` prop at all on
`RecallOverlay`'s own `ItemPanel` call means the microtask affordances, the
streaming narration, the stamp badge and the decline are still reachable
only through Now's or Triage's own item panel — neither of which any capture
in this file opens (Now's per-screen and `now-columns-*` captures never
select a card; Triage's board capture only asserts its header count), not
because either is structurally unreachable under the default `?demo` (board,
since #455 both are real and wired there). **Now's
captures in the columns** join that list for the same reason one more level
up: nothing in this file selects a capture's card, so its `triage` chip, its
place under a column's startable actions and the `TriageRow` editor selecting
it opens stay unphotographed on Now even though the board world mounts them
all — the project dossier's own `-slot-open-*` capture now covers the slot,
but over a minted action, not a capture.
And `shell/SeamFailure.tsx` (ADR-0025, #141/M1-1) — the surface `main.tsx`
renders *instead of* `App` when the main-thread decision seam fails to
instantiate. No query flag can reach it: it requires a wasm failure, which is
what the surface exists to report. `shell/SeamFailure.test.tsx` is the cover.
`screens/NowScreen.test.tsx` and `screens/TriageScreen.test.tsx` are the
cover; a board mixing both kinds is reviewed by hand on a device with real
captures, which is where a full inbox exists at all. **The Grill takeover**
(#355, ADR-0023) joins that same exclusion for a related reason:
`TriageRow`'s real "Grill me" button, the question card, the review card and
every turn state (asking, the question, the proposal, a decline) only exist
over a real `TaskItemDTO`. Since #456 `TriageScreen` has no kit-fixture branch
left to fall back to, so the rows are always real and the button is always
real — but nothing in this file clicks it, so the takeover still never opens
under `pnpm visual`. `screens/GrillTakeover.test.tsx`,
`screens/TriageScreen.test.tsx` and the `shell/useGrillWiring.ts` /
`shell/useGrillTakeoverWiring.ts` hook tests are the cover for every
reachable turn state, and round 2's own tests cover the refused-Confirm
error path (a `needs_re_review` answer leaves the takeover standing and
names itself on the review card). What is still unphotographed is how any
of it *looks*: the not-ready, disconnected and error states still owe
#355's acceptance its own hand pass on a device with a real foggy
capture — not yet performed as of this PR, and worth doing before or
shortly after this lands, since none of it is exercised by `pnpm visual`.
**The draft's own two surfaces join the same exclusion, for the same
reason** (#356): the takeover's **Discard** button — the app's first
`window.confirm` dialog, so its native chrome is unstyled and unphotographed
by construction, not merely by the demo gate — and a Triage row's
**Resume grill** label (`item-actions.ts`'s `grillButtonLabel`, replacing
"Grill me" once `TaskState.grillDraftItemIds` names the item) both only
render over a real, drafted `TaskItemDTO`. `screens/GrillTakeover.test.tsx`
covers the Discard confirm/cancel branches and `item-actions.test.ts`
covers the label function; a hand pass confirming how the confirm dialog
and the resumed label actually read is owed alongside #355's own.
**Now's frontier columns are reached by the default `?demo`** (ADR-0021
decision 8, #400, closed by #420's board world and #455's flip of which
spelling is the default) — `now-columns-*` proves the wrap at production
density, and the per-screen `now-*` capture proves the ordinary default view.
Still unphotographed by `pnpm visual`, because the capture is one still frame
of the default view rather than a sequence of interactions: the axis switch,
the facet-filter panel, and the collapsed/`n more` states. Those take #273's
disposition — `screens/NowScreen.test.tsx` plus the grouping and preference
modules' own unit tests are the cover, and the wrap behaviour, the collapse
reflow and the absence of horizontal page overflow are reviewed by hand at
1440, 1024 and 768 in both themes on a device with real items, which is where
enough columns to wrap exist at all.

**Now's own Grill takeover (#359) joins that same exclusion, for a related
reason** — settled here rather than discovered mid-slice, per that issue's
own instruction not to let this be a mid-slice surprise. It is
the frontier board's all the way down: the takeover only opens over a real
`TaskItemDTO` behind Now's centre-column card (`ItemPanel`'s `"detail"` mode's
own "Grill me"/"Resume grill" button, gated by `item-actions.ts`'s widened
`canGrill`). The default `?demo` (board, since #455) does mount the board
over real `TaskItemDTO`s, so this branch is *reachable* under the gate's own
world now — but nothing in `visual/surfaces.spec.ts` clicks "Grill me", so it
stays unphotographed in practice, same disposition every other Grill/frontier
surface above already took: component tests are the cover, not `pnpm visual`.
`screens/NowScreen.test.tsx`'s "the Grill takeover (#359)" suite covers
opening it from Now's own button, the takeover replacing the centre column
while the ordinary board is gone, Back closing it and restoring focus to the
button that opened it, and — the one property this surface has that Triage's
identical takeover does not — the standing-questions aside staying mounted
throughout, asserted directly rather than eyeballed. The turn states
themselves (asking, question, proposal, decline), the Confirm/Keep grilling
buttons and the Discard confirm dialog are `screens/GrillTakeover.test.tsx`'s
existing cover, reused as-is since the component is identical on both
screens. What is still unphotographed, exactly as for Triage's own takeover:
how any of it *looks*. Now's version of the not-ready, disconnected and error
states owes its own hand pass on a device with a real foggy Ready or In
Progress action — not yet performed as of this PR, and the same debt #355
already recorded for Triage's identical states.

**The aside's *collapsed* state is photographed by no gate at all** — and it
is worth naming rather than leaving to be discovered, because unlike the other
gaps above it is not a fixture problem but a preference one. `openApp` seeds
only `hb.theme` into `localStorage`, so every run starts with
`hb.questions.aside-collapsed` absent, which `aside-prefs.ts` reads as open;
every Now capture in both worlds, at all four widths and both themes, shows the
panel standing and the header's chevron. Nothing photographs the header's `?`,
the vanished panel, or the centre column at its widened measure — which is the
one of the three that could actually go wrong at a width, since it is the
columns reflowing into space they never have under the gate. Seeding the key
would cost one init script and is the obvious fix, but it doubles the Now
matrix to buy one still frame, so it is deferred rather than rejected: this is
a real gap with a cheap close, not a decided exclusion like the board's
own was. `screens/NowScreen.test.tsx` (open by default, and no empty landmark when
shut), `shell/Header.test.tsx` (both glyph states, the callback, its absence on
every screen but Now, and the Refresh/questions/New ordering that keeps the
button still) and `questions/aside-prefs.test.ts` are the cover for the
behaviour; how the shut screen *looks* is reviewed by hand.

**What the popover capture contains has changed shape, and one part of it has
dropped out of reach.** The chrome row that used to sit above the capture
field is gone: the close X moved into the field's own trailing slot beside the
dictation microphone (`screens/CaptureBox.tsx` — a row carrying nothing but an
X was taller than the field it sat over, and the field is the whole reason the
popover opens), so `CapturePopover` draws no control of its own and hands the
box an `onClose`. The "More details" disclosure lost its text label — the glyph
carries it, with `label` serving as both accessible name and tooltip — and now
rides at the right-hand end of the Energy/Size/Context row, held level with the
Context select by `.hb-capture-details-toggle` in `shell/responsive.css`; at
390px, where those three fields stack, that same class pins it to the foot of
the stack instead, so it lands beside Context rather than marooned against the
Energy slider's track. Both forms of that alignment are therefore decidable
only from the captures, and the `phone` project is the one that proves the
second. **The disclosure's open state is not photographed**: `detailsOpen`
starts `false` and the spec screenshots the popover as it opens, so the mint
fields behind it — Description, Project, Priority, the deadline control and
Scheduled date — appear in no capture. `shell/CapturePopover.test.tsx` (which
drives the disclosure open and asserts every revealed field onto the submit)
and `components/forms/DeadlineField.test.tsx` are the cover, and the expanded card
is worth a hand pass at 390 in particular, where it is the tallest thing the
popover can become.

**The capture popover's dictation microphone (#379) is unphotographed, and no
capture will ever contain it — not the resting mic, and not one of its
lifecycle states.** The ordinary "Dictate" mic renders only where local speech
recognition has been *confirmed* — `available({langs:["en-US"],
processLocally: true})` answering `"available"` (ADR-0022) — which is a
browser with the on-device pack installed; #381's setup mic (and its
explain-then-download control) renders instead for `"downloadable"` /
`"downloading"`. **The gate deletes the speech constructor before the app
loads** (`openApp`'s second init script), so neither arm is reached at all —
`isDictationApiPresent()` reads false and the capability never leaves
`unsupported`, which is ADR-0022's ordinary ship-nothing arm. That is not
tidiness: headless Chromium 151.0.7922.34 **crashes the renderer** when
`SpeechRecognition.available()` is called, so mounting the capture box killed
the tab and all eight capture-popover cases failed on the click that opens it.
Measured both ways in the same build — headless crashes, `headless: false`
returns `"downloadable"`. Deleting the constructor is not pixel-neutral: a
real headed browser resolves `"downloadable"` to `setup-required` and renders
#381's setup mic, but the gate's headless run (constructor deleted)
pins the capability at `unsupported`, which renders nothing. The two are
different states rendered for different reasons — the gate accepts that gap
because the setup mic is not photographable here regardless (see below), and
a renderer crash is not catchable from the page that provokes it, which is
why the defence is in the harness and not a `navigator.webdriver` check in
product code.

**None of the states past "nothing renders" are photographable, and none of
them ever will be**, since they all need either a live microphone or an
on-device model download, neither of which headless Chromium has: the
ordinary listening mic (`active` treatment, "Stop dictating"), #381's
explain/download hint in its three phases (explained, installing, failed),
the on-device-model-installed re-probe to `ready`, and a dictation error —
including a denied-microphone ("Microphone access is blocked for this site.")
— stated in the field's own alert paragraph. Every one of those is covered
instead by `screens/CaptureBox.dictation.test.tsx` (the seam mocked
wholesale, which is the only way any gate here reaches the listening or
setup states), `speech/local-dictation.test.ts` (the error-code-to-message
map, including `not-allowed`) and `screens/capture-dictation.test.ts`, and
reviewed by hand on the desk Chrome — the only browser this repo has
measured any of it on, and — until the phone is probed — the only one any of
it is known to appear in at all.

**Since #420 the columns, the captures among them and #418's stranded-write
alerts ARE photographed** (both of them — the board fixture seeds a failed
triage and a failed act, and `surfaces.spec.ts` asserts the count rather than
the first match, which is what caught the second line arriving) — the twelfth
state, `now-columns-*`, and the reason the count above moved (it has since
moved several times more: to sixteen for #481's four Recall states below, to
eighteen for #637's `rules-list-*` and #624's `projects-dossier-*`, to
nineteen for #647's `capture-context-list-*`, to twenty-three for the
projects-dossier batch's four dossier states, back to twenty-two when
the dossier's centre column became the frontier board: `-action-expanded-*`
and `-action-no-steps-*` went with the action list they photographed,
`-no-actions-*` became `-empty-board-*`, and `-slot-open-*` arrived, and to
twenty-three for the Status board's `status-expanded-*`). Not by widening the kit world to mount the board, which the
rejection below still refuses: by a **second demo world**, the board, which
seeds a real `TaskState` (`src/fixtures/demo-task-state.ts`) and
returns `null` for `DemoData`. `NowScreen` renders the board
unconditionally since #456 deleted its `demo` prop and the branch it gated
(and since the project dossier began sharing it, that board is
`screens/FrontierBoard.tsx` rather than a component private to Now);
the board world is now selected by `demoTaskState()` seeding `task` in
`App.tsx:114-115`. #420 shipped the board world at the explicit spelling
`?demo=board`, alongside the kit world's existing bare `?demo`; **#455
flipped which spelling is the default** — the board is now what bare `?demo`
(and every spelling but `?demo=kit`) means, and it is the primary nine-screen
capture pass below, not a second pass alongside a kit one. The rejection above
is intact — this is the "decided change with its reasoning written down" that
ADR-0021 decision 8 named as its own condition, and that decision (as amended
by #420 and again by #455) carries the record. The fixture mirrors
**production's measured shape and none of its content** (29 cards, its
context/size/energy/source spread, no blocked edges), so what the gate
photographs is the real awkward board rather than a tidy one. **Projects is
the one measured number it deliberately does not mirror** (#624): production
holds none, and a faithful mirror would hand the Projects grid an empty list
and photograph an empty screen — so `demo-task-state.ts`'s departure 4 seeds
three, one archived, and the board assertion names a card rather than
asserting the grid rendered. Both of that screen's levels are photographed:
the grid on the per-screen sweep, and the dossier — reachable only by a
click, since it opens on the screen's own local state — by its own
`projects-dossier-*` capture, which is what proves the two-column skeleton
and the back affordance survive the phone form. **Two more of the dossier's
own states join it**, each reached by a click the capture drives itself:
`projects-dossier-slot-open-*` (the board's selected-item slot, expanded
above the columns inside a project's narrower centre column) and
`projects-dossier-archive-confirm-*` (#630's confirm dialog, the one state
that names a live-item count). `projects-dossier-empty-board-*` is a third,
in its own `test()` block: the board's "Nothing startable in this project"
state, reached on the fixture's archived project — the one with no items at
all, which is also what the archive cascade produces (ADR-0030 decision 5),
so that capture covers the archived dossier and its `archived` badge too.
**This is where the frontier board's interaction states are photographed at
all**: the selected-card slot is covered here rather than on Now, whose own
per-screen capture is one still frame of the default view. Still uncovered
on either surface and still on the disposition: the axis switch, the
facet-filter panel and the collapse reflow. The
popover is a state rather than a screen — it renders
over whatever is showing (`shell/CapturePopover.tsx`), so no per-screen
capture ever contains it, and the scrim covering the whole window plus the
card fitting inside 768 are only decidable with it open. The default `?demo`
drives the *real* ranked region through the board seed's own `bindings` /
`paneReads` (`src/fixtures/demo-pane-reads.ts`, built inside
`demo-task-state.ts`'s `buildDemoTaskState()` — a bound waste question
collecting tomorrow at the address, so what is photographed is an answered,
imminent pane, plus a bound `f1` race question twelve days out, the `distant`
state the race pane holds for most of the year); there is deliberately no
demo-only rendering of the region, so the capture is the shipping component.
(Before #455 this ran through a third, hand-authored fixture world,
`demo-questions.ts`, that fed the kit world's own Now/Status captures; #452
folded its content into the board seed and #455 deleted the module along with
the kit-world captures it fed — see ADR-0021 decision 8's own amendment.) The
empty states matter on their own: they are what a new device actually shows,
and no fixture screen exercises them. **Rules is populated under the default
`?demo` too**, from the board seed's own `rules` / `kindRegistry` (#452,
folded from the kit world's `ruleDetails` / `ruleKindRegistry` —
`demo-data.ts`, #140), so its capture is a deterministic, populated rules
screen — condition rows, toggles and a backtest count — the same as the other
five. `RulesScreen` carries no separate kit branch any more either (#457
deleted `App.tsx`'s `demo ? demo.ruleDetails : task.rules` — and the sibling
ternaries over `kindRegistry`/`frontier`/`lastRuleWrite`/`onCreateRule`/
`onPatchRule` — alongside `DemoData` itself), so this board capture is now
Rules' only render path, `?demo=kit` included.

**Alerts lost its only capture at #455's flip; #457 closed the
component-test half of the gap it left.** The screen does not read
`TaskState` at all — it calls its own dev-gated `demoData()` directly
(`fixtures/demo-data.ts`), never a `demo` prop from `App.tsx` — so under the
default `?demo` (board) it still always renders the honest empty state an
unseeded device would (`alerts renders and asserts the seed`), and its only
populated render, the kit fixture's rule cards, stays reachable solely at
`?demo=kit`, which nothing in `visual/surfaces.spec.ts` opens.
`AlertsScreen.test.tsx` (#457) now covers both states — the honest empty
render, the kit-populated render, and the kit fixture standing down in a
production build — so this screen's populated state is no longer reviewed by
hand only; a capture pass restored to `visual/surfaces.spec.ts` would still
be the only way to photograph it, and nothing in this file adds one. Routes
was the second such screen until #624 deleted it; `ProjectsScreen`, which
replaced it, reads `TaskState` on every world and is photographed on the
per-screen sweep plus three dossier captures (above).

**Now, Triage, Settings and Rules carry no kit rendering at all any more**
(#456, #457): each screen's fixture-only block or branch (Now's hero card and
"Also startable" list, Triage's fixture card list and "swept every 15m" meta,
Settings' acked-alerts switch and its inert "Mirror" section, Rules' `demo ?`
ternaries) is deleted along with the screen's own `demo` prop or argument and
the `App.tsx` guards that only existed to keep writes inert under it — all
four take their real render path unconditionally now, `?demo=kit` included.
Unlike Alerts (still kit-fed above), there is no populated kit-only render
left on any of the four to review by hand.

**Recall (#478–#481) is photographed under the BOARD world, not the kit
one** — the reason is the kit world's `task`, not the trigger: since #456
every trigger #480 wired (the header's Search button, the `/` hotkey, the
rail's magnifier, the phone More sheet's entry) fires unconditionally on
every world (`App.tsx`'s `onSearch={requestSearchOpen}`). What still confines
this to the board world is `task` itself: `demoTaskState()` (`fixtures/demo.ts`)
seeds a `TaskState` only for the board spelling, so under `?demo=kit` `task`
is `liveTask`, the real store slice, with no seeded `search` answer to
photograph — a deterministic capture needs the board world's fixed seed, not
a live round trip. (Before #456 the trigger itself was also inert under
`?demo=kit`, and `onTriage` was absent in kit mode everywhere else; #456
deleted both ternaries, so only `task`'s own difference between the two
worlds still confines Recall's populated render to board.) The board
world seeds an answer of its
own for the identical structural reason `now-columns`'s alerts and
`triageInbox` are seeded rather than requested: `App.tsx`'s
`task = demoTask ?? liveTask` means `task.search` never falls through to
whatever the real worker answers, even though `useRecallWiring` still fires
the request — `fixtures/demo-task-state.ts`'s `search` seed (one row per
group Recall groups by: `b-f7` live, `b-d2` done, `b-a2` archived) is the
only answer the overlay will ever render under `?demo=board`, whatever query
is actually typed. Four captures per width and theme: the results listed
(all three seeded rows, so one capture proves live/done/archived render
together), the live row expanded with its edit form (`onTriage` reaches the
live group only, wired to the real `handleTriage` unconditionally since
#456), the archived row expanded read-only (no Edit affordance — the same
absence a `"done"` result gets, `RecallRow`'s own rule, so photographing one
of the two stands for both), and the overlay open on an empty query ("Type
to search", the one state that needs no seed at all). `visual/surfaces.spec.ts`'s
`openRecall` helper is the one trigger used — the header's Search button,
mounted identically on all four projects.

There are **no committed golden images and no pixel diff.** The project has
no baseline history, and a pixel gate with nobody to arbitrate it produces
noise rather than findings. The captures are the deliverable — review them
for clipping, overlap, broken wrapping, and sticky/scroll or focus glitches.
What the spec *does* fail on is the machine-decidable subset: horizontal
overflow at any width, unresolved brand tokens, a theme switch that does not
reach the page, and (#453) the page loading a different demo world than the
one `openApp` asked for.

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

## Surface: native Android

**Built, and deliberately ungated here.** `client/android/` has had a code
root since M0 (#141) and now renders thirteen screens — the nine the bottom
nav and its More sheet reach (Now, Status, Alerts, Rules, Triage, Done,
Ledger, Settings, Routes; #532/#541 — **Status is the design handoff's
"quiet stack" since #689**: announcing panes as accent cards in the seam's
order, everything else folded into one card of 44dp chips whose detail opens
in place, which is ADR-0017 decision 1 executed rather than the web's tile
board of ADR-0033), Recall (#542), and three deeper
destinations a row or a notification opens (alert detail, item detail, the
Grill takeover) — plus the capture box and the notification lane's own shade
UI. Several have grown well past a list of rows: capture is a full field set
since #529 (destination, size, energy, context, description, a project picker
over the live project list, priority, deadline, scheduled date), Now is the
frontier board in the phone's single-column form since #530 (switchable
grouping axis, facet chips, per-column collapse, a blocked section, axis and
collapse state persisted through DataStore) with the now-surface panes below
its queue since #537, and Status is the ranked region's second surface
(#536). None of it is photographed by anything.

That is a real gap, not an oversight to fix casually: `android.yml` runs no
emulator (its own header says why), so a screenshot matrix has to be stood
up before it can gate anything.

**Settled 2026-08-20, operator decision: visual evidence for this surface is
a real device or an emulator. A Robolectric render is not evidence.** This
had been open since #576 put Robolectric and `compose-ui-test-junit4` in the
module — the substrate a screenshot gate (Roborazzi and its kin) runs on
*without* an emulator — and it sat unmoved through four sessions of Android
visual work, because accumulating evidence cannot answer a question about
what counts as evidence. It is answered now, and the answer closes the
Roborazzi route rather than choosing it: the reason is that the defects this
row exists to catch have repeatedly been ones the Robolectric substrate is
blind to by construction. The clearest case is 2026-08-20's capture sheet,
where making it open full height put the title field's outline against the
status-bar clock — a window-inset defect, invisible to a render that has no
window, and caught only by screenshotting the phone.

So the evidence below is **not** an interim arrangement pending a decision;
it is the permanent, named floor beneath a gate that does not exist yet. Each
kind is named so nobody mistakes it for a visual gate:

- **Structural tests**, JVM, reading the Compose source as text —
  `NavigationStructuralTest`, `BottomNavStructuralTest`,
  `NowScreenStructuralTest`, `AlertsScreenStructuralTest`,
  `StatusScreenStructuralTest`, `SettingsScreenStructuralTest`,
  `RulesScreenStructuralTest`, `TriageScreenStructuralTest`,
  `ItemDetailPanelStructuralTest` (the pane all four item-detail hosts
  render, unified 2026-08-20), `RecallScreenStructuralTest`,
  `CaptureSubmitRefusalTest`,
  `CaptureFieldSetStructuralTest`, `ScreenStateRetentionTest`,
  `GrillTakeoverStructuralTest` (#595), and the component-scoped
  `StageBadgeTest`/`LevelGlyphsTest`/`GlyphRenderTest` (#557/#558, which mix
  source pins with Robolectric semantics renders). They answer
  "is it wired the way the ADRs say", never "does it look right" — and a
  screen added without one of these leaves this surface with no evidence at
  all, so add the test with the screen.
- **`ColorTokenDriftTest`** (#483, ADR-0026), which pins `ui/theme/Color.kt`
  against the design system's token CSS. It is about appearance, but it
  covers the palette only — never a screen.
- **`ChoiceRowWrappingTest`** (#576), **`FacetLabelAlignmentTest`** (#588),
  **`AxisRowWrappingTest`** and **`PriorityRowWrappingTest`** (both the
  round-4/round-5 operator batches, 2026-08-20) — the four things here that
  measure **layout**: real Compose renders under Robolectric, asserting a
  row of choices stays hittable, a facet label seats beside its first chip
  line, and the axis strip and the priority row each fit one line. Each
  covers one component shape plus a negative control rendering its defect —
  never a screen's whole composition, and they photograph nothing.
  The later two moved the qualifier off 320dp deliberately: the operator
  ruled the budget is **the device's** width, the Fold's 443dp cover
  display, not a synthetic stress width (`AxisRow`'s own header carries the
  measurements and the accepted clipping limit). Their
  `@GraphicsMode(NATIVE)` is load-bearing: without it Robolectric measures
  text with a stub and any assertion here passes vacuously, which is the
  trap to know before writing the next test of this kind. A second thing is
  load-bearing beside it, and is known only because it shipped a clipped
  chip: **the content must be wrapped in `HummingbirdTheme`.** A bare render
  resolves `MaterialTheme.typography` to Material's defaults rather than the
  app's bundled faces, which measured an axis strip green at 268dp while the
  device clipped its trailing chip. A further edge found at #558:
  `captureToImage()` **times out** under this Robolectric setup even in
  NATIVE mode, so pixel-level assertions are off the table — bounds
  measurement is the ceiling of what this substrate gives. That ceiling was
  one input to the 2026-08-20 decision above, and it is now moot rather than
  open: these tests measure component *layout*, they are not a visual gate,
  and no amount of them will become one.
- **Hardware runs**, recorded in `client/android/README.md`'s "Proving the
  lane on hardware". Operator-run, not CI, and the only evidence any of
  these screens has ever rendered — which the 2026-08-20 decision makes the
  *right* kind of evidence, merely an unautomated one. Until the emulator
  matrix exists, a device pass is what a UI change on this surface owes.

Add the emulator matrix here when one exists — that is now the only route to
gating this row, per the 2026-08-20 decision above, and standing one up in
`android.yml` is the whole of the remaining work. Do not quietly treat the
structural or bounds-measuring tests as covering it; they were never the
blocker and are not the answer.

## Planned, not built

The design system carries UI kits for **Wear OS** and **iOS**
(`.claude/skills/hummingbird-design/ui_kits/`). Neither has code in this
repo, so neither has a gate here. Add a surface section when one gets a code
root — an emulator/simulator matrix, per the `/wrapup` reference.

---

## What component tests cover, and what this does not

The visual gate answers "does it look right". It does not answer "is it
wired" — three of the four PRs in the S10–S13 batch shipped UI state with no
reader, all of which rendered fine. That gap is covered by the component
tests (`client/web/src/**/*.test.tsx`, jsdom, run by `pnpm test`) and by
`client/web/src/test/component.tsx`, which explains the failure mode. Neither
gate substitutes for the other.
