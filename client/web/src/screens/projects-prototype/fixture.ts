// PROTOTYPE (#449) — throwaway. Fixture world for the Projects page UX
// exploration; the whole `projects-prototype/` directory is deleted once a
// variant wins. Shapes mirror the plan's schema (github_repo as canonical
// owner/repo, default_context copy-at-mint, timestamp-matched cascade
// archive) but nothing here is a wire DTO.

export type ProtoStage = "ready" | "in_progress" | "blocked" | "done";

export interface ProtoStep {
  id: string;
  body: string;
  done: boolean;
}

export interface ProtoAction {
  id: string;
  title: string;
  stage: ProtoStage;
  context: string | null;
  steps: ProtoStep[];
  /** Millisecond stamp; the cascade gives every live item the project's
   * exact value, so unarchive can restore only what archive took down. */
  archivedAt: number | null;
}

export interface ProtoFog {
  id: string;
  question: string;
  resolvedAt: number | null;
}

export interface ProtoLink {
  id: string;
  url: string;
  label: string | null;
}

export interface ProtoProject {
  id: string;
  name: string;
  /** Canonical `owner/repo`; the URL is derived for display, never stored. */
  githubRepo: string | null;
  defaultContext: string | null;
  archivedAt: number | null;
  route: { destination: string; notes: string };
  fog: ProtoFog[];
  links: ProtoLink[];
  /** Array order is project_pos. */
  actions: ProtoAction[];
}

export const CONTEXTS = ["@home", "@computer", "@phone", "@errands", "@garden", "@waiting"];

export function repoUrl(repo: string): string {
  return `https://github.com/${repo}`;
}

export function liveActions(project: ProtoProject): ProtoAction[] {
  return project.actions.filter((action) => action.archivedAt === null);
}

/** The mono meta counts line: `4 ACTIONS · 1 FOG`. */
export function countsMeta(project: ProtoProject): string {
  const actions = liveActions(project).filter((action) => action.stage !== "done").length;
  const fog = project.fog.filter((entry) => entry.resolvedAt === null).length;
  const parts = [`${actions} action${actions === 1 ? "" : "s"}`];
  if (fog > 0) parts.push(`${fog} fog`);
  return parts.join(" · ");
}

// The Kitchen remodel is pre-archived, and one of its actions carries an
// OLDER stamp than the project — it was archived on its own beforehand, so
// unarchiving the project must leave it down. That is the honesty the
// warning dialog states.
const KITCHEN_ARCHIVED_AT = 1754500000000;
const KITCHEN_EARLIER = 1751000000000;

export function seedProjects(): ProtoProject[] {
  return [
    {
      id: "proj-greenhouse",
      name: "Greenhouse",
      githubRepo: null,
      defaultContext: "@garden",
      archivedAt: null,
      route: {
        destination: "Seedlings hardened off and planted out before the last frost date.",
        notes: "The cold frame is the bottleneck — check the forecast before committing a weekend.",
      },
      fog: [
        { id: "fog-g1", question: "Is the west bed drainage actually fixed?", resolvedAt: null },
        {
          id: "fog-g2",
          question: "Which tomato varieties are worth the space this year?",
          resolvedAt: 1754300000000,
        },
      ],
      links: [
        { id: "lnk-g1", url: "https://example.com/sensor-manual", label: "sensor manual" },
      ],
      actions: [
        {
          id: "act-g1",
          title: "Order the replacement sensor",
          stage: "ready",
          context: "@computer",
          archivedAt: null,
          steps: [
            { id: "stp-g1", body: "Find the model number on the old unit", done: true },
            { id: "stp-g2", body: "Check the supplier still lists it", done: true },
            { id: "stp-g3", body: "Place the order", done: false },
          ],
        },
        {
          id: "act-g2",
          title: "Hear back from the shop about the part",
          stage: "blocked",
          context: "@waiting",
          archivedAt: null,
          steps: [],
        },
        {
          id: "act-g3",
          title: "Rebuild the cold frame lid",
          stage: "ready",
          context: "@garden",
          archivedAt: null,
          steps: [],
        },
        {
          id: "act-g4",
          title: "Move the seedlings out",
          stage: "ready",
          context: null,
          archivedAt: null,
          steps: [],
        },
      ],
    },
    {
      id: "proj-hummingbird",
      name: "Hummingbird",
      githubRepo: "JddAndrewLauren/hummingbird",
      defaultContext: "@computer",
      archivedAt: null,
      route: {
        destination: "The project lane is a real surface — creatable, editable, archivable from the UI.",
        notes: "Server side is mostly done; the gap is client-side.",
      },
      fog: [
        {
          id: "fog-h1",
          question: "Does action reorder need optimistic projection, or is round-trip fine?",
          resolvedAt: null,
        },
      ],
      links: [
        { id: "lnk-h1", url: "https://github.com/JddAndrewLauren/hummingbird/issues/449", label: "plan issue" },
        { id: "lnk-h2", url: "https://hb.twinion.net", label: "the authority" },
      ],
      actions: [
        {
          id: "act-h1",
          title: "Rewrite the sweeper's Gmail adapter",
          stage: "in_progress",
          context: "@computer",
          archivedAt: null,
          steps: [
            { id: "stp-h1", body: "Open the fixture generator script", done: true },
            { id: "stp-h2", body: "Regenerate the Gmail fixture set", done: true },
            { id: "stp-h3", body: "Run the adapter tests once", done: true },
            { id: "stp-h4", body: "Delete the two dead label cases", done: false },
            { id: "stp-h5", body: "Re-read the sweeper doc's invariants", done: false },
          ],
        },
        {
          id: "act-h2",
          title: "Draft the ADR for shared Route ownership",
          stage: "ready",
          context: "@computer",
          archivedAt: null,
          steps: [],
        },
      ],
    },
    {
      id: "proj-house",
      name: "House",
      githubRepo: null,
      defaultContext: "@home",
      archivedAt: null,
      route: {
        destination: "The boiler serviced and the insurance renewed before winter.",
        notes: "",
      },
      fog: [],
      links: [],
      actions: [
        {
          id: "act-o1",
          title: "Book the annual boiler service",
          stage: "ready",
          context: "@phone",
          archivedAt: null,
          steps: [],
        },
        {
          id: "act-o2",
          title: "File the insurance renewal",
          stage: "done",
          context: null,
          archivedAt: null,
          steps: [],
        },
      ],
    },
    {
      id: "proj-kitchen",
      name: "Kitchen remodel",
      githubRepo: null,
      defaultContext: "@home",
      archivedAt: KITCHEN_ARCHIVED_AT,
      route: {
        destination: "A usable kitchen with the wall opened up.",
        notes: "Shelved until the quote comes back under budget.",
      },
      fog: [{ id: "fog-k1", question: "Load-bearing or not?", resolvedAt: null }],
      links: [],
      actions: [
        {
          id: "act-k1",
          title: "Get a second structural quote",
          stage: "ready",
          context: "@phone",
          archivedAt: KITCHEN_ARCHIVED_AT,
          steps: [],
        },
        {
          id: "act-k2",
          title: "Measure the wall cavity",
          stage: "done",
          context: null,
          // Archived on its own, before the project — a project unarchive
          // must leave this one down (its stamp does not match).
          archivedAt: KITCHEN_EARLIER,
          steps: [],
        },
      ],
    },
  ];
}
