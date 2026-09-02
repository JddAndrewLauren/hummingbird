import type { IconName } from "../../components/core/Icon";
import type { RankedPane, StandingQuestion } from "../questions/contract";
import { paneKey } from "../questions/contract";

// The Status board's *layout* vocabulary (the design handoff's Status
// screen): which of the two labelled grids a pane lands in, and which glyph
// identifies it.
//
// This is the one datum on this surface that no decision produces. Bands,
// answer states, headlines and facts all arrive decided from the core
// (ADR-0025) — but "the model-credit question belongs beside the uptime
// probes, under the word `infra`" is a statement about how a reader scans a
// board, which is rendering. So it lives here as a literal table in the view
// layer: not a field on `QuestionDef` (`surface` is the declared axis
// ADR-0017 bought, and a second one would need its own decision), and not a
// value read through the wasm seam at module-evaluation time, which throws
// before the core is initialised (ADR-0025 as amended by #500/#501).
//
// Every entry is a fallback away from breaking. A question missing from
// `QUESTION_TILES` still renders, in `infra`, with its group's default
// glyph; a subject missing from `SUBJECT_ICONS` — a renamed workflow, a
// newly probed service — takes its question's default. The board's job is to
// draw whatever `rankPanes` returned, so an unrecognised pane is a tile with
// a duller icon, never a hole. `tile-vocabulary.test.ts` pins the questions
// (which are declared here in the repo) and exercises the subject fallback
// (which is server data, and cannot be).

/** The two grids, in the order the board draws them. */
export type StatusGroup = "infra" | "capture & context sources";

export const STATUS_GROUPS: readonly StatusGroup[] = [
  "infra",
  "capture & context sources",
];

interface TileVocabulary {
  group: StatusGroup;
  /** Drawn when the pane's own subject has no entry in `SUBJECT_ICONS`. */
  icon: IconName;
}

/** Per-question group and default glyph. `Partial` because the table is
 * about this surface's questions: Now's five have no place on this board,
 * and `tileGroup`/`tileIcon` answer for anything absent. */
const QUESTION_TILES: Partial<Record<StandingQuestion, TileVocabulary>> = {
  kimi: { group: "infra", icon: "circle-dollar-sign" },
  uptime: { group: "infra", icon: "server" },
  reachability: { group: "infra", icon: "smartphone" },
  // Every GitHub pane is one scheduled workflow, and every workflow this
  // repo schedules is a capture or context poller — so the question sits in
  // the second group and its subjects carry the source's own glyph below.
  // `git-branch`, not `github`: Lucide dropped its brand marks, and
  // `Icon.tsx` records that substitution.
  github: { group: "capture & context sources", icon: "git-branch" },
  // A meta-question over every other source's own freshness — `infra`
  // beside kimi/uptime/reachability. `radio`, not `activity` (the fallback
  // this table's own drift pin refuses to let a registered question keep):
  // this pane's whole subject is whether a source is still broadcasting.
  poller: { group: "infra", icon: "radio" },
};

/** Per-subject glyphs, keyed by `paneKey` so a subject name can never be
 * mistaken for another question's. The uptime services are
 * `server`/`globe`/`cpu` — the authority, the web origin and the Fly
 * runner; the workflows carry the source they poll. */
const SUBJECT_ICONS: Record<string, IconName> = {
  [paneKey("uptime", "authority")]: "server",
  [paneKey("uptime", "web")]: "globe",
  [paneKey("uptime", "runner")]: "cpu",
  [paneKey("github", "gmail-poll.yml")]: "mail",
  [paneKey("github", "graph-mail-poll.yml")]: "mails",
  [paneKey("github", "calendar-poll.yml")]: "calendar",
  [paneKey("github", "graph-calendar-poll.yml")]: "calendar",
  [paneKey("github", "city-waste-poll.yml")]: "trash-2",
  [paneKey("github", "race-alert-poll.yml")]: "flag",
};

/** Which grid this pane draws into — `infra` for anything unregistered, so
 * a new question appears on the board before it appears in this table. */
export function tileGroup(pane: RankedPane): StatusGroup {
  return QUESTION_TILES[pane.question]?.group ?? "infra";
}

/** This pane's glyph: its subject's, else its question's, else the neutral
 * `activity`. */
export function tileIcon(pane: RankedPane): IconName {
  return (
    SUBJECT_ICONS[pane.paneKey] ??
    QUESTION_TILES[pane.question]?.icon ??
    "activity"
  );
}
