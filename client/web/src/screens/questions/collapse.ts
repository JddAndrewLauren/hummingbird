import type { Band, PaneAnswer } from "./contract";

// ADR-0015's **band-scoped, device-local collapse** (#245): whether a pane is
// showing its whole answer or just its one-line collapsed row.
//
// Two decisions are worth stating up front, because both are deliberate:
//
// **This is not a binding.** A collapse is a view preference on one device,
// not a cross-device fact, so it belongs in the injectable-`storage` idiom
// `calendar/persistence.ts` and `theme/theme.ts` already use — never in the
// `settings` table, which has no DELETE and syncs to every device (ADR-0015
// says this explicitly).
//
// **The override is scoped to the band it was made in.** "I collapsed the
// waste pane" means "…while it was dormant", not "…forever": the next time
// the collection is tomorrow, the pane is imminent and must open again on
// its own. A stored override therefore applies only while the pane's
// computed band still matches the band it was stored against — and a
// mismatch is a *read-time non-match*, never a deletion, which is what makes
// the round trip dormant -> imminent -> dormant **resurrect** the original
// override instead of quietly losing it.

const COLLAPSE_KEY = "hb.questions.collapse";

/** The narrow slice of `localStorage` this module needs — the same
 * injectable shape `calendar/persistence.ts` defines, redeclared here rather
 * than imported so nothing about the questions shell depends on the calendar
 * lane. */
export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/** One stored override: which way, and the band it was made in. Both halves
 * are load-bearing — the direction, because a reader may equally want a
 * dormant pane *open* (the collapse default is a default, not a rule), and
 * the band, because that is the scope. */
export interface CollapseOverride {
  band: Band;
  collapsed: boolean;
}

export type CollapseMap = Record<string, CollapseOverride>;

const BANDS: readonly string[] = ["live", "imminent", "near", "distant", "dormant"];

function isOverride(value: unknown): value is CollapseOverride {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { band?: unknown; collapsed?: unknown };
  return typeof candidate.band === "string" && BANDS.includes(candidate.band) &&
    typeof candidate.collapsed === "boolean";
}

/** Reads the stored map. Anything unreadable — absent, not JSON, not an
 * object, an entry a newer build wrote in a shape this one cannot use —
 * reads as `{}`: a view preference is never worth an error, and the default
 * rule below is always a correct answer. */
export function readCollapseMap(storage: StorageLike): CollapseMap {
  const raw = storage.getItem(COLLAPSE_KEY);
  if (!raw) {
    return {};
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      return {};
    }
    const map: CollapseMap = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (isOverride(value)) {
        map[key] = { band: value.band, collapsed: value.collapsed };
      }
    }
    return map;
  } catch {
    return {};
  }
}

function writeCollapseMap(storage: StorageLike, map: CollapseMap): void {
  storage.setItem(COLLAPSE_KEY, JSON.stringify(map));
}

/** The shell's default when nobody has said otherwise: collapsed when the
 * pane is dormant, or when it has no answer to show.
 *
 * A gap and an unbound question are collapsed for the same reason a dormant
 * pane is — none of the three is what the reader came for — but collapsed is
 * not *hidden*: the collapsed row still says which question it is and what
 * state it is in, which is what keeps "visibly broken, never quietly empty"
 * true of a pane with no data at all. */
export function defaultCollapsed(answer: PaneAnswer): boolean {
  return answer.band === "dormant" || answer.answerState !== "answered";
}

/** Whether this pane renders collapsed right now: the stored override when
 * it was made in the band the pane is in *now*, and the default rule
 * otherwise. */
export function resolveCollapsed(
  stored: CollapseMap,
  key: string,
  answer: PaneAnswer,
): boolean {
  const override = stored[key];
  if (override !== undefined && override.band === answer.band) {
    return override.collapsed;
  }
  return defaultCollapsed(answer);
}

/** Records one override and returns the map to store and hold in state.
 *
 * Two things happen here and nowhere else. The override is stamped with the
 * band it was made in — that is what scopes it. And the map is **pruned of
 * panes that are no longer ranked at all**: without it, a device that once
 * had a question this build no longer registers would carry its entry
 * forever. Band-mismatched entries are deliberately *kept* — they are the
 * resurrection case, not stale data. */
export function writeCollapseOverride(
  storage: StorageLike,
  current: CollapseMap,
  key: string,
  override: CollapseOverride,
  livePaneKeys: readonly string[],
): CollapseMap {
  const live = new Set(livePaneKeys);
  const next: CollapseMap = {};
  for (const [paneKey, entry] of Object.entries(current)) {
    if (live.has(paneKey)) {
      next[paneKey] = entry;
    }
  }
  next[key] = override;
  writeCollapseMap(storage, next);
  return next;
}
