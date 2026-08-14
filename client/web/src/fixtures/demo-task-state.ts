// The **board** demo world (#420): a seeded `TaskState` — the exact shape the
// sync engine publishes — so the screens take their REAL render path with
// fictional data in it.
//
// Why this exists at all. The kit world (`demo-data.ts`) cannot reach Now's
// centre column: `NowScreen` branches to `RealFrontier` only when `demo` is
// null, so the frontier's columns, the unsorted captures among them, the axis
// switch, the Filter panel and #418's stranded-triage alert have never been
// photographed by the visual gate — ADR-0021 decision 8 recorded that gap and
// this closes it. `DemoItem` could not have expressed them anyway: it carries
// no `context` and no `energy`, having been written before either was an axis.
//
// **Nothing here is real data.** Every title, description and source below is
// invented. What *is* taken from production is the SHAPE — measured once from
// `GET /api/changes?since=0` on 2026-08-13, when the authority held 37 items:
//
//   board cards      29  (12 frontier + 17 unsorted captures)
//   by context       no context 12 · @computer 8 · @errands 4 · @phone 3 · @home 2
//   by size          no size 13 · deep 8 · quick 4 · short 4
//   by energy        no energy 17 · high 5 · medium 4 · low 3
//   by source        typed 21 · gmail/v1 7 · google-tasks/v1 1
//   projects          0        blocked_by  0        priority  all 0
//
// The mirror is deliberate down to the awkward parts, because those are the
// findings: **the no-value bucket is the biggest column on every axis** (and
// ADR-0021 decision 1 pins it always-last, so the largest column sits past the
// fold), **two context columns exceed `COLUMN_CAP`** so `n more` is the normal
// case rather than an edge one, and **grouping by Project yields exactly one
// column** because production has no projects at all. A tidier fixture would
// photograph a system nobody has.
//
// Two deliberate departures, both so the gate keeps covering states production
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

import type { TaskItemDTO } from "../store/protocol";
import type { TaskState } from "../store/store";

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
    projectId: null,
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

/** The startable twelve. Contexts here plus the captures' below sum to
 * production's own spread exactly — see the header's table. */
const FRONTIER_SEEDS: Seed[] = [
  {
    id: "b-f1",
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
    size: "short",
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
    size: "short",
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
    size: "short",
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
    size: "short",
  },
];

/** The seeded state, typed as the real `TaskState` so a field added to that
 * interface fails this file at build time rather than shipping a fixture that
 * silently omits it. Every "not read yet" field is left at its honest `null` —
 * the board world seeds the frontier and the inbox, and claims nothing about
 * the Ledger, Done, bindings or rules, which have their own screens.
 *
 * A **function**, not a const, and that is the bundling requirement in the
 * header: a const would have to be constructed at import, which is a top-level
 * side effect, which is what kept 5.3 KB of this file in the production bundle
 * the first time round. Called once, from behind `demo.ts`'s dead-branch gate.
 * Reading the clock here rather than at import is the free improvement that
 * falls out: ages are relative to when the board is asked for. */
export function buildDemoTaskState(): TaskState {
  const loadedAt = Date.now();
  return {
    frontier: FRONTIER_SEEDS.map((seed, index) => item(seed, index, loadedAt)),
    triageInbox: TRIAGE_SEEDS.map((seed, index) => item(seed, index, loadedAt)),
    // Production has no `blocked_by` edges and no projects at all — the second
    // is why grouping by Project produces exactly one column here, which is a
    // finding about the axis rather than a gap in the fixture.
    blocked: [],
    stepsByItem: {},
    projects: [],
    ledger: null,
    done: null,
    bindings: null,
    kindRegistry: null,
    rules: null,
    lastRuleWrite: null,
    paneReads: {},
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
    lastBindingWrite: null,
    lastSyncOutcome: null,
    lastSyncAtMs: null,
    lastSuccessfulSyncAtMs: null,
    syncOutcomeSeq: 0,
    queueDepth: null,
    deadLetters: [],
    needsReconnect: false,
    hostError: null,
  };
}
