// The **board** demo world (#420): a seeded `TaskState` — the exact shape the
// sync engine publishes — so the screens take their REAL render path with
// fictional data in it.
//
// Why this exists at all. The kit world (`demo-data.ts`) cannot reach Now's
// centre column: until #456, `NowScreen` branched to `RealFrontier` only
// when `demo` was null, so the frontier's columns, the unsorted captures
// among them, the axis switch, the Filter panel and #418's stranded-triage
// alert had never been photographed by the visual gate — ADR-0021 decision 8
// recorded that gap and this closes it. `DemoItem` could not have expressed
// them anyway: it carries no `context` and no `energy`, having been written
// before either was an axis.
//
// *Amended 2026-08-20 (#456): `NowScreen` deleted its `demo` prop and the
// branch above with it — it renders `RealFrontier` unconditionally now, on
// every world. The board world above is unaffected: it is still what this
// module seeds and still the only populated render of Now's centre column
// the visual gate photographs.*
//
// **Nothing here is real data.** Every title, description and source below is
// invented. What *is* taken from production is the SHAPE — measured once from
// `GET /api/changes?since=0` on 2026-08-13, when the authority held 37 items:
//
//   board cards      29  (12 frontier + 17 captured) — before departure 3 below
//                        adds one fictional Grilling item, never measured
//   by context       no context 12 · @computer 8 · @errands 4 · @phone 3 · @home 2
//   by size          no size 13 · deep 8 · quick 4 · normal 4
//   by energy        no energy 17 · high 5 · medium 4 · low 3
//   by source        typed 21 · gmail/v1 7 · google-tasks/v1 1
//   projects          0        blocked_by  0        priority  all 0
//
//   (`projects 0` is what production measured; departure 4 below seeds three,
//   so this fixture no longer mirrors that number.)
//
// The mirror is deliberate down to the awkward parts, because those are the
// findings: **the no-value bucket is the biggest column on every axis** (and
// ADR-0021 decision 1 pins it always-last, so the largest column sits past the
// fold), and **two context columns exceed `COLUMN_CAP`** so `n more` is the
// normal case rather than an edge one. A tidier fixture would photograph a
// system nobody has. (Grouping by Project used to yield exactly one column
// here, for the same faithfulness reason; departure 4 ended that — see it for
// why the Projects grid outranked the axis.)
//
// Four deliberate departures, all so the gate keeps covering states production
// happens not to be in today:
//
//   1. Production holds ONE deadline, so a faithful mirror would paint every
//      card `calm` and the urgency ladder — the card's only licensed colour
//      (ADR-0021 decision 2) — would go unphotographed. Three items carry
//      deadlines here, one per band: overdue, now, soon.
//   2. `lastTriage` AND `lastAct` are each seeded with a FAILURE, so both of
//      Now's stranded-write alerts render (#418 and its twin). They are the
//      only things on this board always on screen that would not normally be;
//      without them the lines those fixes added are invisible to the gate
//      exactly as the columns were, and seeding just one would photograph a
//      surface that looks like it has a single failure slot when it has one
//      per mutation kind. Expect both when eyeballing `?demo=board` by hand —
//      they are the fixture, not a real fault.
//   3. Production has never grilled anything into Grilling yet, so a faithful
//      mirror would leave the "triage process" queue's second half (#357,
//      CONTEXT.md) unphotographed by `visual/surfaces.spec.ts:224`'s "now's
//      columns" capture — exactly the surface this fixture exists to cover.
//      One item is seeded `stage: "grilling"` so its own `StageBadge` and its
//      place in `triageProcessQueue`'s combined order both render somewhere
//      the gate actually looks.
//   4. Production holds NO projects (see the table above), so a faithful
//      mirror would hand `?demo=board`'s Projects grid (#624) an empty list
//      and the visual gate would photograph an empty screen and pass — a
//      failure mode this repo has a documented history of. `PROJECT_SEEDS`
//      seeds three, one of them archived so the grid's Show-archived toggle
//      is reachable at all, and five existing board items carry a
//      `projectId` so the cards' action counts are derived rather than zero.
//      The cost is the finding this fixture used to carry about the Project
//      axis: grouping by Project now yields three columns plus the no-value
//      one, not the single column production would give. That trade was
//      taken deliberately — an unphotographed screen is a worse gap than an
//      over-populated axis.
//
// **#452 grows this seed to the whole of `TaskState`, not just the frontier
// and the inbox**, so every screen that reads the store — not only Now —
// takes its real render path under `?demo=board`. Four pieces:
//
//   1. `rules` / `kindRegistry` — the kit world's `ruleDetails` /
//      `ruleKindRegistry` (`demo-data.ts`), reused rather than re-typed: they
//      are already the real wire DTOs `RulesScreen` reads. `projects` and
//      `blocked` stay `[]`, unchanged — production's own measured shape
//      (above), not a gap.
//   2. `done` / `ledger` — measured from the same `GET /api/changes?since=0`
//      call, re-run on 2026-08-14, when the authority held 46 items:
//
//        by stage     triage 22 · ready 14 · done 7 · grilling 2 · in_progress 1
//        done          6 of 7 carry no size/energy; 1 is deep/high
//        done source   3 none · 3 gmail/v1 · 1 google-tasks/v1
//        archived      6, spread across other stages (not `done`) —
//                      the mirror's retention stamp outliving a stage
//
//      `DONE_SEEDS` below mirrors that split at a smaller n (6 items: 5 no
//      size/energy split none/gmail/google-tasks, 1 deep/high) rather than
//      copying the 7 real ones verbatim — the measured SHAPE travels, never
//      the real titles. `buildDemoTaskState`'s `ledger` is the frontier, the
//      inbox, `grillingItems` (departure 3's seeded Grilling item) and `done`
//      unioned into `LedgerRowDTO`s (the same "one item pool, several reads
//      of it" the real mirror gives, via `liveLedgerRow`) — every live list
//      this fixture builds, not a subset of them — plus `ARCHIVED_ONLY_SEEDS`
//      standing in for the six archived rows the count above found — again
//      the shape at a smaller n (three), not the real six. That is
//      12 + 17 + 1 + 6 + 3 = 39 Ledger rows in total. One live row carries
//      `deadLettered: true` and one carries `hasLiveAlert: true`
//      (`DEAD_LETTERED_ITEM_ID`/`LIVE_ALERT_ITEM_ID`), so both of the
//      Ledger's badges render.
//   3. `bindings` / `paneReads` / (`lastSyncOutcome`, `lastSyncAtMs`,
//      `lastSuccessfulSyncAtMs`) — `demo-questions.ts` used to hand-build
//      these same six `QuestionInputs` fields as a THIRD fixture world,
//      duplicating the two above. Every one is expressible in `TaskState`, so
//      it is expressed here instead — `demo-pane-reads.ts` holds the actual
//      `PaneReadDTO` builders now, shared by both worlds. `calendarReads: {}`
//      and `calendarConnected: false` need no seeding at all: they come
//      straight off the live `CalendarState` (`App.tsx`'s prop threading,
//      unchanged), and a board device with no calendar credential mounted
//      honestly reads that way. `bindings` gains a `trips-calendar` entry
//      beside the waste/race ones `demo-pane-reads.ts` already carries —
//      production's own three known settings keys, measured from the same
//      call (`city-waste-page`, `trips-calendar`, `race-series`, all known).
//   4. A demo calendar for Settings' calendar card — `demo-calendar.ts`,
//      threaded in by `App.tsx` at the PROP boundary, not through this
//      module: see that file's header for why `CalendarState` cannot be
//      overridden the way this one is.
//
// Dev-only, gated twice, same as the kit world — see `demo.ts`.
//
// **Everything below is built inside a function, and the seed arrays are inert
// data.** That is a bundling requirement, not a style: the dead-branch gate in
// `demo.ts` only removes this fixture if Rollup can prove the module is
// side-effect-free at the top level. The first cut of this file failed that —
// a top-level `Date.now()` plus `deadlineIn(...)` CALLS inside the array
// literals meant Rollup had to retain the seeds, and 5.3 KB of fixture shipped
// in the production bundle while the kit world's pure literal was dropped. So:
// no top-level clock read, and offsets in the seeds are plain numbers that the
// builder turns into strings. `pnpm assert-no-fixtures` is the regression
// test, and it runs in CI after the build.

import { TRIPS_CALENDAR_BINDING_KEY } from "../calendar/selection";
import type { BindingDTO, LedgerRowDTO, ProjectDTO, RecallRowDTO, TaskItemDTO } from "../store/protocol";
import type { TaskState } from "../store/store";
import { DEMO_DATA } from "./demo-data";
import {
  boundRaceBinding,
  boundWasteBinding,
  githubRead,
  GITHUB_SOURCE,
  kimiRead,
  KIMI_SOURCE,
  raceRead,
  RACE_SOURCE,
  uptimeRead,
  UPTIME_SOURCE,
  wasteRead,
  WASTE_SOURCE,
} from "./demo-pane-reads";

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

interface Seed {
  id: string;
  title: string;
  stage: TaskItemDTO["stage"];
  /** How long before *now* this was captured. Resolved against the clock when
   * the state is built, never frozen — `demo-data.ts`'s `deadlineHoursFromNow`
   * established the reason and it is the same one here: a fixed timestamp
   * drifts out of its urgency band and silently stops demonstrating the state
   * it exists to demonstrate, and a capture captured in 2026 would read
   * "412d ago" forever. */
  agoMs: number;
  context?: string;
  size?: TaskItemDTO["size"];
  energy?: TaskItemDTO["energy"];
  /** Offsets from now, resolved by the builder. Numbers rather than the
   * strings they become, so these literals stay inert — see the header. */
  deadlineInMs?: number;
  scheduledInMs?: number;
  description?: string;
  source?: string;
  /** Departure 4 (see the header): the project this item belongs to. */
  projectId?: string;
}

/** `YYYY-MM-DDTHH:MM`, naive local — the only deadline spelling ADR-0009/0013
 * allows, and what `screens/urgency.ts` parses back as local wall-clock. */
function deadlineAt(loadedAt: number, ms: number): string {
  const d = new Date(loadedAt + ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function item(seed: Seed, index: number, loadedAt: number): TaskItemDTO {
  const createdAt = loadedAt - seed.agoMs;
  return {
    id: seed.id,
    seq: index + 1,
    title: seed.title,
    description: seed.description ?? null,
    stage: seed.stage,
    size: seed.size ?? null,
    energy: seed.energy ?? null,
    context: seed.context ?? null,
    // Production carries priority 0 on every item — nothing has ever been
    // prioritised — so the fixture does too.
    priority: 0,
    projectId: seed.projectId ?? null,
    projectPos: null,
    deadline: seed.deadlineInMs === undefined ? null : deadlineAt(loadedAt, seed.deadlineInMs),
    scheduledDate:
      seed.scheduledInMs === undefined
        ? null
        : deadlineAt(loadedAt, seed.scheduledInMs).slice(0, 10),
    source: seed.source ?? null,
    sourceKey: seed.source ? `${seed.id}-key` : null,
    sourceUrl: null,
    archivedAt: null,
    createdAt,
    updatedAt: createdAt,
    version: 1,
    pending: false,
  };
}

/** Departure 4 (see the header): three projects, one archived. Production
 * holds none, so a faithful mirror would photograph an empty Projects grid —
 * and this repo has a documented history of a gate passing on an empty
 * screen. `archivedAt` on the third is what makes the grid's Show-archived
 * toggle photographable at all.
 *
 * `agoMs` becomes a real stamp in `project()` below, for the same
 * no-top-level-clock reason the item seeds carry offsets rather than dates. */
const PROJECT_SEEDS: { id: string; name: string; agoMs: number; archived?: boolean }[] = [
  { id: "b-p1", name: "House repairs", agoMs: 30 * DAY },
  { id: "b-p2", name: "Autumn garden clear-up", agoMs: 12 * DAY },
  { id: "b-p3", name: "Sell the old bike", agoMs: 90 * DAY, archived: true },
];

function project(seed: (typeof PROJECT_SEEDS)[number], loadedAt: number): ProjectDTO {
  const createdAt = loadedAt - seed.agoMs;
  return {
    id: seed.id,
    name: seed.name,
    githubRepo: null,
    defaultContext: null,
    archivedAt: seed.archived === true ? createdAt + DAY : null,
    createdAt,
    updatedAt: createdAt,
    version: 1,
  };
}

/** The startable twelve. Contexts here plus the captures' below sum to
 * production's own spread exactly — see the header's table. */
const FRONTIER_SEEDS: Seed[] = [
  {
    id: "b-f1",
    projectId: "b-p1",
    title: "Fit the new tap washer",
    stage: "ready",
    agoMs: 5 * DAY,
    context: "@home",
    size: "quick",
    energy: "low",
  },
  {
    id: "b-f2",
    title: "Reply to the letting agent",
    stage: "ready",
    agoMs: 3 * DAY,
    context: "@computer",
    size: "normal",
    energy: "medium",
    // Band 1 of 3: overdue.
    deadlineInMs: -6 * HOUR,
  },
  {
    id: "b-f3",
    title: "Renew the car insurance",
    stage: "ready",
    agoMs: 9 * DAY,
    context: "@computer",
    size: "quick",
    energy: "low",
    // Band 2 of 3: inside the 24h "now" window.
    deadlineInMs: 8 * HOUR,
  },
  {
    id: "b-f4",
    title: "Draft the loft insulation quote request",
    stage: "ready",
    agoMs: 2 * DAY,
    context: "@computer",
    size: "deep",
    energy: "high",
    // Band 3 of 3: past 24h, inside the 3-day "soon" window.
    deadlineInMs: 40 * HOUR,
  },
  {
    id: "b-f5",
    title: "Take the recycling to the tip",
    stage: "ready",
    agoMs: 6 * DAY,
    context: "@errands",
    size: "normal",
    energy: "medium",
    scheduledInMs: 2 * DAY,
  },
  {
    id: "b-f6",
    title: "Ring the dentist about the crown",
    stage: "ready",
    agoMs: 11 * DAY,
    context: "@phone",
    size: "quick",
    energy: "low",
  },
  {
    id: "b-f7",
    title: "Rewrite the backup script so it prunes old snapshots",
    stage: "ready",
    agoMs: 14 * DAY,
    context: "@computer",
    size: "deep",
    energy: "high",
    description:
      "The nightly job has never deleted anything, so the disk fills every few months and the fix is always manual.",
  },
  {
    id: "b-f8",
    title: "Clear the gutters before the storms",
    stage: "ready",
    agoMs: 8 * DAY,
    size: "deep",
    energy: "high",
  },
  {
    id: "b-f9",
    projectId: "b-p2",
    title: "Sort the shed shelves",
    stage: "ready",
    agoMs: 21 * DAY,
    size: "deep",
    energy: "medium",
  },
  {
    id: "b-f10",
    title: "Pick up the dry cleaning",
    stage: "ready",
    agoMs: 4 * DAY,
    context: "@errands",
    size: "quick",
  },
  {
    id: "b-f11",
    title: "Read through the pension statement",
    stage: "ready",
    agoMs: 17 * DAY,
    size: "deep",
    energy: "high",
    description: "Two pages of it are the same table twice; the change is somewhere in the third.",
  },
  {
    id: "b-f12",
    title: "Update the household inventory spreadsheet",
    stage: "in_progress",
    agoMs: 30 * DAY,
    size: "normal",
    energy: "medium",
    description: "Started before the holiday and abandoned about a third of the way down.",
  },
];

/** The unsorted seventeen. Captures read the way captures actually read —
 * lowercase, half-finished, mostly unset — because production's do: 13 of
 * these carry no size at all and 16 no energy, which is what makes the
 * no-value column the biggest one on the board. */
const TRIAGE_SEEDS: Seed[] = [
  { id: "b-t1", title: "Milk", stage: "triage", agoMs: 25 * MINUTE },
  {
    id: "b-t2",
    title: "the back gate is dragging again",
    stage: "triage",
    agoMs: 2 * HOUR,
    context: "@home",
  },
  {
    id: "b-t3",
    title: "ask about the trailer hitch",
    stage: "triage",
    agoMs: 5 * HOUR,
    context: "@phone",
  },
  {
    id: "b-t4",
    projectId: "b-p1",
    title: "Re: your quote for the glazing",
    stage: "triage",
    agoMs: 7 * HOUR,
    context: "@computer",
    source: "gmail/v1",
  },
  {
    id: "b-t5",
    title: "check whether the boiler service is due or they are just chasing again",
    stage: "triage",
    agoMs: 9 * HOUR,
    size: "deep",
    description: "Last one was either eleven or twenty-three months ago depending on which email.",
  },
  {
    id: "b-t6",
    title: "Fwd: parents evening — dates to confirm",
    stage: "triage",
    agoMs: 14 * HOUR,
    source: "gmail/v1",
  },
  { id: "b-t7", title: "passport expiry??", stage: "triage", agoMs: 20 * HOUR },
  {
    id: "b-t8",
    title: "Re: invoice 20641",
    stage: "triage",
    agoMs: 26 * HOUR,
    context: "@computer",
    source: "gmail/v1",
  },
  {
    id: "b-t9",
    title: "bins go out thursday not wednesday this week",
    stage: "triage",
    agoMs: 30 * HOUR,
  },
  {
    id: "b-t10",
    projectId: "b-p2",
    title: "Re: gutter cleaning availability",
    stage: "triage",
    agoMs: 2 * DAY,
    source: "gmail/v1",
  },
  {
    id: "b-t11",
    title: "look into the loft hatch ladder",
    stage: "triage",
    agoMs: 3 * DAY,
    size: "deep",
    source: "google-tasks/v1",
  },
  {
    id: "b-t12",
    title: "Re: your order has shipped",
    stage: "triage",
    agoMs: 4 * DAY,
    context: "@computer",
    source: "gmail/v1",
  },
  {
    id: "b-t13",
    title: "sort out the photo backups properly",
    stage: "triage",
    agoMs: 5 * DAY,
    context: "@computer",
    size: "deep",
    energy: "high",
    description: "Three half-copies on two disks and neither is the one the phone syncs to.",
  },
  {
    id: "b-t14",
    projectId: "b-p2",
    title: "Re: hedge trimmer service booking",
    stage: "triage",
    agoMs: 6 * DAY,
    context: "@errands",
    source: "gmail/v1",
  },
  {
    id: "b-t15",
    title: "Re: council tax band review",
    stage: "triage",
    agoMs: 8 * DAY,
    source: "gmail/v1",
  },
  {
    id: "b-t16",
    title: "return the library books",
    stage: "triage",
    agoMs: 12 * DAY,
    context: "@errands",
  },
  {
    id: "b-t17",
    title: "ring the vet about the booster",
    stage: "triage",
    agoMs: 16 * DAY,
    context: "@phone",
    size: "normal",
  },
];

/** Departure 3 in the header: the one item already grilled once and still
 * foggy — `TaskState.grillingItems`, `Core::grilling_items`'s (#357) own
 * query. Sorts ahead of `TRIAGE_SEEDS` in `triageProcessQueue`'s combined
 * order, behind any local draft (none seeded here — a draft is device-local
 * state this fixture makes no claim about). */
const GRILLING_SEEDS: Seed[] = [
  {
    id: "b-g1",
    title: "book the removals van",
    stage: "grilling",
    agoMs: 10 * HOUR,
    size: "deep",
    description: "Grilled once already; still can't settle on a date range.",
  },
];

/** The six done items (#452, piece 2) — measured shape in the header: five
 * of six carry no size/energy, one is deep/high, sources split
 * none/gmail/google-tasks. */
const DONE_SEEDS: Seed[] = [
  {
    id: "b-d1",
    title: "File the storm drain report with the city",
    stage: "done",
    agoMs: 10 * DAY,
  },
  {
    id: "b-d2",
    title: "Reply to the HOA about the fence colour",
    stage: "done",
    agoMs: 6 * DAY,
    source: "gmail/v1",
  },
  {
    id: "b-d3",
    title: "Confirm the plumber's invoice",
    stage: "done",
    agoMs: 15 * DAY,
    source: "gmail/v1",
  },
  {
    id: "b-d4",
    title: "Pay the storage unit renewal",
    stage: "done",
    agoMs: 20 * DAY,
    source: "google-tasks/v1",
  },
  {
    id: "b-d5",
    title: "Update the emergency contact card",
    stage: "done",
    agoMs: 25 * DAY,
  },
  {
    id: "b-d6",
    title: "Rebuild the compost bin from the split boards",
    stage: "done",
    agoMs: 12 * DAY,
    size: "deep",
    energy: "high",
  },
];

/** Three archived-only ledger rows — not on the frontier, in the inbox or
 * among `DONE_SEEDS`, standing in (at a smaller n) for the six the header's
 * measurement found spread across other stages: the mirror's retention
 * stamp outliving a stage, exactly what `ledgerRowState` labels "archived"
 * regardless of what `stage` the row still carries. */
interface ArchivedSeed {
  id: string;
  title: string;
  stage: TaskItemDTO["stage"];
  agoArchivedMs: number;
}

const ARCHIVED_ONLY_SEEDS: ArchivedSeed[] = [
  { id: "b-a1", title: "Cancel the storage unit direct debit", stage: "ready", agoArchivedMs: 40 * DAY },
  { id: "b-a2", title: "Chase the warranty claim", stage: "triage", agoArchivedMs: 22 * DAY },
  { id: "b-a3", title: "Book the chimney sweep", stage: "in_progress", agoArchivedMs: 60 * DAY },
];

function archivedLedgerRow(seed: ArchivedSeed, index: number, loadedAt: number): LedgerRowDTO {
  const archivedAt = loadedAt - seed.agoArchivedMs;
  return {
    id: seed.id,
    seq: 200 + index,
    title: seed.title,
    description: null,
    stage: seed.stage,
    size: null,
    energy: null,
    context: null,
    priority: 0,
    projectId: null,
    projectPos: null,
    deadline: null,
    scheduledDate: null,
    source: null,
    sourceKey: null,
    sourceUrl: null,
    archivedAt,
    createdAt: archivedAt - 5 * DAY,
    updatedAt: archivedAt,
    version: 1,
    pending: false,
    absentSinceMs: archivedAt,
    deadLettered: false,
    hasLiveAlert: false,
  };
}

/** One `RecallRowDTO` (#478) over an already-built `TaskItemDTO` — a search
 * result is the item plus which of the three groups it fell into, never a
 * second copy of the item's own fields (`RecallRowDTO extends TaskItemDTO`,
 * `store/protocol.ts`). Used below to stand in for `Core::search`'s answer,
 * which #481's board fixture needs seeded: `task.search` never falls
 * through to the live store's own answer in board mode (`App.tsx`'s
 * `task = demoTask ?? liveTask`, and `demoTask` is never `null` here), so
 * whatever `useRecallWiring` requests live is discarded and this seed is the
 * only search answer the overlay will ever render under `?demo=board`. */
function recallRow(taskItem: TaskItemDTO, group: RecallRowDTO["group"]): RecallRowDTO {
  return { ...taskItem, group };
}

/** A live `TaskItemDTO` (frontier, inbox or `DONE_SEEDS`), read as the
 * Ledger's own shape — same item, one more read of it, never a second
 * source of truth (`ledger-order.ts`'s own "derive, don't record"— the
 * screen's, borrowed here since the fixture is the same shape). `deadLettered`
 * and `hasLiveAlert` default false; `DEAD_LETTERED_ITEM_ID`/
 * `LIVE_ALERT_ITEM_ID` below name the one row of each that carries the
 * badge, so the Ledger screen's two badges both render at least once. */
function liveLedgerRow(
  taskItem: TaskItemDTO,
  badges: { deadLettered?: boolean; hasLiveAlert?: boolean } = {},
): LedgerRowDTO {
  return {
    ...taskItem,
    absentSinceMs: null,
    deadLettered: badges.deadLettered ?? false,
    hasLiveAlert: badges.hasLiveAlert ?? false,
  };
}

/** Which live row demonstrates which Ledger badge — named by id so the
 * choice is legible beside the rows themselves rather than threaded through
 * index arithmetic. `b-f2` ("Reply to the letting agent") already carries a
 * deadline band; `b-t4` ("Re: your quote for the glazing") is already a
 * `gmail/v1` capture — neither badge invents a new item to hang itself on. */
const DEAD_LETTERED_ITEM_ID = "b-f2";
const LIVE_ALERT_ITEM_ID = "b-t4";

const boundTripsBinding: BindingDTO = {
  key: TRIPS_CALENDAR_BINDING_KEY,
  known: true,
  pending: false,
  // The same fictional calendar id `demo-calendar.ts` lists — a board device
  // with a Trips calendar designated locks that same row in Settings'
  // picker, which is what makes the two fixtures agree the way `demo-data.ts`
  // and `demo-questions.ts` already had to.
  value: { state: "text", text: "demo-family" },
};

/** The seeded state, typed as the real `TaskState` so a field added to that
 * interface fails this file at build time rather than shipping a fixture that
 * silently omits it.
 *
 * #452 grew this from "the frontier and the inbox, nothing else" to the
 * whole shape a synced board device actually holds — the header's four
 * pieces. What is STILL left at its honest `null`/`{}`/`false` is exactly
 * what a real board device would not have answered either:
 * `lastRuleWrite`/`lastGrillCompletion`/`lastBindingWrite` (no write has
 * been issued this session), `pending`/`stepsByItem` (nothing has been
 * asked about yet), and `queueDepth`/`deadLetters`/`needsReconnect`/
 * `hostError` (S9's sync-status affordance, outside this issue's four
 * pieces).
 *
 * A **function**, not a const, and that is the bundling requirement in the
 * header: a const would have to be constructed at import, which is a top-level
 * side effect, which is what kept 5.3 KB of this file in the production bundle
 * the first time round. Called once, from behind `demo.ts`'s dead-branch gate.
 * Reading the clock here rather than at import is the free improvement that
 * falls out: ages are relative to when the board is asked for. */
export function buildDemoTaskState(): TaskState {
  const loadedAt = Date.now();
  const frontier = FRONTIER_SEEDS.map((seed, index) => item(seed, index, loadedAt));
  const triageInbox = TRIAGE_SEEDS.map((seed, index) => item(seed, index, loadedAt));
  const done = DONE_SEEDS.map((seed, index) => item(seed, index, loadedAt));
  // Departure 3 in the header: production has never grilled anything into
  // Grilling, so this seeds one anyway — the surface this fixture exists
  // to keep photographed. Hoisted alongside `frontier`/`triageInbox`/`done`
  // (not inlined at its one call site below) because the Ledger's pool and
  // `activeItemCount` both need the same read of it — "one item pool,
  // several reads of it" holds across all four live lists, not just three.
  const grillingItems = GRILLING_SEEDS.map((seed, index) => item(seed, index, loadedAt));

  const ledger: LedgerRowDTO[] = [
    ...[...frontier, ...triageInbox, ...grillingItems, ...done].map((taskItem) =>
      liveLedgerRow(taskItem, {
        deadLettered: taskItem.id === DEAD_LETTERED_ITEM_ID,
        hasLiveAlert: taskItem.id === LIVE_ALERT_ITEM_ID,
      }),
    ),
    ...ARCHIVED_ONLY_SEEDS.map((seed, index) => archivedLedgerRow(seed, index, loadedAt)),
  ];

  // #481's Recall search seed — one row per group `RecallOverlay` renders
  // differently: `b-f7` ("Rewrite the backup script…", `stage: "ready"`) is
  // the LIVE row, carrying a description so its expanded `ItemPanel` has
  // more than a title to show and wired with `onTriage` (`App.tsx` passes
  // `handleTriage` through unconditionally since #456) so its expansion
  // offers Edit; `b-d2` ("Reply to the HOA…") is the DONE row
  // and `b-a2` ("Chase the warranty claim", one of `ARCHIVED_ONLY_SEEDS`) is
  // the ARCHIVED row — both expand read-only, since `RecallRow` only ever
  // passes `onTriage` through for a `"live"` group. `archivedLedgerRow`'s
  // three Ledger-only fields (`absentSinceMs`/`deadLettered`/`hasLiveAlert`)
  // are dropped rather than carried into a `RecallRowDTO`, which has no use
  // for them.
  const { absentSinceMs: _archivedAbsentSinceMs, deadLettered: _archivedDeadLettered, hasLiveAlert: _archivedHasLiveAlert, ...archivedRecallItem } =
    archivedLedgerRow(ARCHIVED_ONLY_SEEDS[1], 1, loadedAt);
  const search: TaskState["search"] = {
    rows: [
      recallRow(frontier[6], "live"),
      recallRow(done[1], "done"),
      recallRow(archivedRecallItem, "archived"),
    ],
    total: 3,
  };

  return {
    frontier,
    triageInbox,
    grillingItems,
    // Production has no `blocked_by` edges, so this stays empty — a finding
    // about the relation rather than a gap in the fixture. `projects` was
    // empty for the same reason and no longer is: see departure 4.
    blocked: [],
    stepsByItem: {},
    // Split exactly as the real answer splits it (#624): an archived project
    // is absent in the mirror and arrives on the `archivedProjects` half, so
    // a fixture that put all three in `projects` would photograph a shape the
    // app never produces — and would hide the very bug this split fixed.
    projects: PROJECT_SEEDS.filter((seed) => seed.archived !== true).map((seed) =>
      project(seed, loadedAt),
    ),
    archivedProjects: PROJECT_SEEDS.filter((seed) => seed.archived === true).map((seed) =>
      project(seed, loadedAt),
    ),
    ledger,
    // #481: the board world's own Recall answer, seeded above — `task.search`
    // never falls through to whatever `Core::search` actually answers in
    // board mode (see `recallRow`'s doc), so the overlay would otherwise
    // render "Searching…" forever for any query typed against `?demo=board`.
    search,
    done,
    bindings: [boundWasteBinding, boundRaceBinding, boundTripsBinding],
    // Moved from `demo-data.ts`'s kit-only `DEMO_DATA`, which already typed
    // these as the real `RuleDTO[]`/`KindRegistryDTO` (piece 1's "a move
    // rather than a rewrite") — the kit world keeps its own reference to the
    // same object via `App.tsx`'s `demo.ruleDetails`/`demo.ruleKindRegistry`.
    kindRegistry: DEMO_DATA.ruleKindRegistry,
    rules: DEMO_DATA.ruleDetails,
    lastRuleWrite: null,
    lastProjectWrite: null,
    // Piece 3: every standing question's read, built by `demo-pane-reads.ts`
    // — the kit world's own `demoQuestionInputs` called the same functions
    // before #452 folded its content into this seed and #455 deleted that
    // module, so this remains one input path, not a third fixture world
    // hand-building the same six fields. The weekend/vacation panes' own
    // `calendarReads`/`calendarConnected` arrive through `App.tsx`'s live
    // `CalendarState` prop threading, unchanged, and need no entry here.
    paneReads: {
      [WASTE_SOURCE]: wasteRead(loadedAt),
      [RACE_SOURCE]: raceRead(loadedAt),
      [KIMI_SOURCE]: kimiRead(loadedAt),
      [GITHUB_SOURCE]: githubRead(loadedAt),
      [UPTIME_SOURCE]: uptimeRead(loadedAt),
    },
    pending: {},
    lastCapture: null,
    // The same departure, on the other mutation: an act failure that outlived
    // its detail panel is #418's twin, and it renders the SECOND of Now's two
    // alert lines. Both are seeded because the pair is the thing worth
    // photographing — one line per result the store holds, never one slot they
    // take turns in — and because a stranded act failure is otherwise reachable
    // only by closing the panel at the right moment.
    lastAct: {
      seed: "demo-board-act-1",
      itemId: "b-f1",
      action: "complete",
      kind: "failed",
      error: "the authority refused that edit",
    },
    // Departure 2 in the header: this is what makes #418's alert render, and it
    // names a capture that is genuinely on the board so the alert can find its
    // title. Not a fault — the fixture.
    lastTriage: {
      seed: "demo-board-triage-1",
      itemId: "b-t7",
      kind: "failed",
      error: "the authority refused that edit",
    },
    lastGrillCompletion: null,
    lastGrillDraftWrite: null,
    grillDraftItemIds: [],
    grillDraftByItem: {},
    lastBindingWrite: null,
    // Piece 3's third field: a recent, ordinary completed cycle — the same
    // reading `reachability.ts`'s pane needs to answer `answered` rather than
    // "never synced on this device", and `activeItemCount` mirrors
    // `SyncMirror::active_item_count`'s own filter (every stage but `Done`,
    // `client/core/src/sync/mirror.rs`) — frontier, inbox AND the seeded
    // Grilling item, not the old fixture's unrelated `12`.
    lastSyncOutcome: {
      kind: "completed",
      retryAfterMs: null,
      activeItemCount: frontier.length + triageInbox.length + grillingItems.length,
      wasFullSweep: false,
      deadLettered: 0,
    },
    lastSyncAtMs: loadedAt - 60_000,
    lastSuccessfulSyncAtMs: loadedAt - 60_000,
    syncOutcomeSeq: 1,
    queueDepth: null,
    deadLetters: [],
    needsReconnect: false,
    hostError: null,
  };
}
