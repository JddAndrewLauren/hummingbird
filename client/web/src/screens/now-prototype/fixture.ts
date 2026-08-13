// PROTOTYPE — throwaway. Delete with the rest of `now-prototype/`.
//
// A denser-than-demo frontier so the grouping/filtering variants can be
// judged against real density. The `?demo` fixture holds six items across
// three projects, which is too thin to tell a lane layout from a board;
// this holds ~26 across six projects, five contexts, three sizes and a
// spread of deadlines and do-dates.
//
// In memory only. Nothing here is written anywhere, and the module is only
// ever reached from the `?variant=` render path, which is itself gated on
// `import.meta.env.DEV`.

import type { ProjectDTO, TaskItemDTO, TaskStageName } from "../../store/protocol";

/** A fixed "today" the relative dates below are written against — replaced
 * with the caller's real `nowMs` day at read time by `prototypeItems`. */
const DAY_MS = 24 * 60 * 60 * 1000;

function isoDay(nowMs: number, offsetDays: number): string {
  const date = new Date(nowMs + offsetDays * DAY_MS);
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

export const PROTOTYPE_PROJECTS: ProjectDTO[] = [
  "Greenhouse",
  "Hummingbird",
  "House",
  "Travel",
  "Bikes",
  "Admin",
].map((name, index) => ({
  id: `proj-${index}`,
  name,
  archivedAt: null,
  createdAt: 0,
  updatedAt: 0,
  version: 1,
}));

const PROJECT_ID: Record<string, string> = Object.fromEntries(
  PROTOTYPE_PROJECTS.map((project) => [project.name, project.id]),
);

interface Seed {
  title: string;
  project: string | null;
  stage?: TaskStageName;
  size?: TaskItemDTO["size"];
  energy?: TaskItemDTO["energy"];
  context?: string;
  priority?: number;
  /** Days from now; `undefined` for none. Negative is overdue. */
  deadlineIn?: number;
  scheduledIn?: number;
}

const SEEDS: Seed[] = [
  // Greenhouse
  { title: "Order the replacement sensor", project: "Greenhouse", size: "quick", energy: "low", context: "@computer", priority: 2, deadlineIn: 1 },
  { title: "Run the overnight temperature log", project: "Greenhouse", size: "short", energy: "medium", context: "@home", priority: 3 },
  { title: "Re-tape the north vent seal", project: "Greenhouse", size: "quick", energy: "medium", context: "@garden", priority: 3, scheduledIn: 0 },
  { title: "Price up a second glazing panel", project: "Greenhouse", size: "short", energy: "low", context: "@computer", priority: 4 },

  // Hummingbird
  { title: "Rewrite the sweeper's Gmail adapter", project: "Hummingbird", stage: "in_progress", size: "deep", energy: "high", context: "@computer", priority: 1, deadlineIn: 0 },
  { title: "Close the ranked-region a11y gaps", project: "Hummingbird", size: "deep", energy: "high", context: "@computer", priority: 2, deadlineIn: 4 },
  { title: "Rotate the runner bearer token", project: "Hummingbird", size: "quick", energy: "low", context: "@computer", priority: 1, deadlineIn: -1 },
  { title: "Write the ADR for the pane registry", project: "Hummingbird", size: "deep", energy: "high", context: "@computer", priority: 3 },
  { title: "Trim the wasm worker's dep tree", project: "Hummingbird", size: "short", energy: "medium", context: "@computer", priority: 4 },

  // House
  { title: "Book the annual boiler service", project: "House", size: "quick", energy: "low", context: "@phone", priority: 2, scheduledIn: 1 },
  { title: "Chase the insurance renewal quote", project: "House", size: "quick", energy: "low", context: "@phone", priority: 2, deadlineIn: 2 },
  { title: "Clear the gutter over the porch", project: "House", size: "short", energy: "high", context: "@home", priority: 4 },
  { title: "Replace the hallway smoke alarm", project: "House", size: "quick", energy: "medium", context: "@home", priority: 3, deadlineIn: 6 },
  { title: "Take the old paint tins to the tip", project: "House", size: "short", energy: "medium", context: "@errands", priority: 4, scheduledIn: 2 },

  // Travel
  { title: "Draft the vacation itinerary", project: "Travel", stage: "in_progress", size: "deep", energy: "medium", context: "@computer", priority: 3, scheduledIn: 0 },
  { title: "Renew the passport", project: "Travel", size: "short", energy: "medium", context: "@errands", priority: 1, deadlineIn: 9 },
  { title: "Confirm the airport parking booking", project: "Travel", size: "quick", energy: "low", context: "@phone", priority: 3, deadlineIn: 3 },

  // Bikes
  { title: "Bleed the rear brake", project: "Bikes", size: "short", energy: "high", context: "@home", priority: 4 },
  { title: "Order a chain and two cassettes", project: "Bikes", size: "quick", energy: "low", context: "@computer", priority: 4 },
  { title: "Enter the autumn hill climb", project: "Bikes", size: "quick", energy: "low", context: "@computer", priority: 2, deadlineIn: 5 },

  // Admin
  { title: "Reconcile last quarter's receipts", project: "Admin", size: "deep", energy: "high", context: "@computer", priority: 2, deadlineIn: 12 },
  { title: "Cancel the unused storage plan", project: "Admin", size: "quick", energy: "low", context: "@computer", priority: 3 },
  { title: "Send the accountant the mileage log", project: "Admin", size: "short", energy: "low", context: "@computer", priority: 2, deadlineIn: 2 },

  // No project
  { title: "Ask dad about the trailer hitch", project: null, size: "quick", energy: "low", context: "@phone", priority: 4 },
  { title: "Find the box with the winter tyres", project: null, size: "short", energy: "medium", context: "@home", priority: 4 },
  { title: "Read the two saved longreads", project: null, size: "deep", energy: "low", priority: 4, scheduledIn: 3 },
];

/** The fixture, resolved against a real clock so its deadlines land where
 * the seeds say relative to today. */
export function prototypeItems(nowMs: number): TaskItemDTO[] {
  return SEEDS.map((seed, index) => ({
    id: `PROTO-${`${index + 1}`.padStart(3, "0")}`,
    seq: index + 1,
    title: seed.title,
    description: null,
    stage: seed.stage ?? "ready",
    size: seed.size ?? null,
    energy: seed.energy ?? null,
    context: seed.context ?? null,
    priority: seed.priority ?? 0,
    projectId: seed.project === null ? null : (PROJECT_ID[seed.project] ?? null),
    projectPos: null,
    deadline: seed.deadlineIn === undefined ? null : isoDay(nowMs, seed.deadlineIn),
    scheduledDate: seed.scheduledIn === undefined ? null : isoDay(nowMs, seed.scheduledIn),
    source: null,
    sourceKey: null,
    sourceUrl: null,
    archivedAt: null,
    createdAt: 0,
    updatedAt: 0,
    version: 1,
    pending: false,
  }));
}
