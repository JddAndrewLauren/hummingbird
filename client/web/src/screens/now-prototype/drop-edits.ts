// THROWAWAY (#801, Phase 1). Which field a drop writes, per axis.
//
// This is a deliberate twin of the module the build will actually ship
// (`screens/frontier-drop.ts`, with tests and a drift gate binding it to the
// Rust `raw_axis_value`). It exists here untested so the *gesture* can be
// judged against real grouping — a variant whose drops did nothing would not
// answer the question the prototype asks. Phase 2 rewrites it properly; do
// not promote this file.

import { computeUrgency, type FrontierAxis, type FrontierColumn } from "../../decisions/seam";
import type { ProjectDTO, TaskItemDTO, TriageEdits } from "../../store/protocol";
import { todayDeadline } from "../capture-meta";

/** `overdue` is the one column no drop may land in: a past deadline is never
 * set deliberately (CONTEXT.md's Deadline entry). Everything else, including
 * every no-value column, is a target. */
export function isDropTarget(axis: FrontierAxis, value: string | null): boolean {
  return !(axis === "urgency" && value === "overdue");
}

/** The whole-day deadline `days` civil days from now, in the wire's
 * `YYYY-MM-DD` form. Local, not UTC, for `todayDeadline`'s own reason — and
 * `setDate` past the month's end rolls over, which is why the arithmetic is
 * done on a `Date` rather than on the milliseconds. */
function dayOffsetDeadline(nowMs: number, days: number): string {
  const d = new Date(nowMs);
  d.setDate(d.getDate() + days);
  return todayDeadline(d.getTime());
}

/** The edit a drop of `item` into `column` makes, or `null` when the drop is
 * refused (`overdue`) or is a no-op (the item is already in that column).
 *
 * One field, with one documented exception: a Project drop also copies the
 * project's `defaultContext` onto a context-less item, which is ADR-0030
 * decision 3's rule and the same thing `buildTriageEdits` already does. */
export function dropEdits(
  axis: FrontierAxis,
  column: FrontierColumn,
  item: TaskItemDTO,
  projects: readonly ProjectDTO[],
  nowMs: number,
): TriageEdits | null {
  const target = column.value;
  if (!isDropTarget(axis, target)) return null;

  if (axis === "context") {
    return (item.context ?? null) === target ? null : { context: target };
  }
  if (axis === "size") {
    return (item.size ?? null) === target ? null : { size: target as TaskItemDTO["size"] };
  }
  if (axis === "energy") {
    return (item.energy ?? null) === target ? null : { energy: target as TaskItemDTO["energy"] };
  }
  if (axis === "project") {
    if ((item.projectId ?? null) === target) return null;
    const edits: TriageEdits = { projectId: target };
    const project = target === null ? undefined : projects.find((p) => p.id === target);
    if (project?.defaultContext && (item.context ?? "").trim() === "") {
      edits.context = project.defaultContext;
    }
    return edits;
  }
  // urgency: the one axis whose drop writes a field other than the one it
  // groups by, because the band is computed from `deadline` against the clock.
  if (computeUrgency(item.deadline, nowMs) === target) return null;
  if (target === "now") return { deadline: todayDeadline(nowMs) };
  // +2, not +1: a day-grained deadline resolves to 23:59, so today+1 still
  // reads `now` late in the day. Today+2 is `soon` at every hour.
  if (target === "soon") return { deadline: dayOffsetDeadline(nowMs, 2) };
  if (target === "calm") return { deadline: null };
  return null;
}

/** What the prototype bar prints after a drop — the item by its title, never
 * `HB-<seq>` (the repo's naming rule), and the field it actually wrote. */
export function describeMove(item: TaskItemDTO, axis: FrontierAxis, edits: TriageEdits): string {
  const pairs = Object.entries(edits).map(([k, v]) => `${k} = ${v === null ? "(cleared)" : String(v)}`);
  return `${item.title} -> ${axis}: ${pairs.join(", ")}`;
}
