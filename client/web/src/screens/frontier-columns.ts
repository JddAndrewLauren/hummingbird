// The frontier's "grouped into columns" display shape (#402, ADR-0021
// decision 1). Replaces `frontier-groups.ts`, whose single project axis is
// generalised here into four: project answers *what does this belong to*,
// while the surface is asking *what can I do right now, from where I am, with
// the time and energy I have* — and the axes that answer that are context,
// size and energy. Project survives as one of the four, so nothing is lost.
//
// The four are not invented for this surface. `CONTEXT.md`'s **Delegation
// axis** entry names them — "the fourth axis, alongside size, energy and
// context" — and the fourth, delegation, is deliberately absent here: it is a
// two-valued marker whose absence *is* the default, so grouping by it yields
// one real column plus "everything else". It stays a chip.
//
// **Pure, and deliberately clockless.** This module never mutates its input,
// returns the same output for the same input, and needs no `nowMs` at all —
// none of the four axes is time-varying, so there is no clock to thread and
// none to read ambiently. Ordering *within* a column is not its business
// either: the caller passes items already through `orderFrontier`, and
// grouping preserves that order inside every bucket, exactly as
// `frontier-groups.ts` did. One ordering rule, one spelling.

import type { ProjectDTO, TaskItemDTO } from "../store/protocol";

/** The axes the frontier can be grouped by (ADR-0021 decision 1). */
export type FrontierAxis = "context" | "project" | "size" | "energy";

/** Every axis, in the order the switch offers them. `context` leads because
 * it is the default: *where you are* rules more work out than anything else
 * on this list. ADR-0021 carries the tripwire — if the axis is in practice
 * never switched off `context`, the switch is the thing to cut. */
export const FRONTIER_AXES: readonly FrontierAxis[] = ["context", "project", "size", "energy"];

export const DEFAULT_FRONTIER_AXIS: FrontierAxis = "context";

export interface FrontierColumn {
  /** The axis value this column collects, or `null` for the bucket of items
   * that name no value on this axis. */
  value: string | null;
  /** `value`'s display name where the axis needs a wire hop — `project`
   * resolves the project's real `name` from `projects`, which is the hop PR
   * #200's review added to `frontier-groups.ts` after an earlier version
   * rendered a raw uuid as a heading. `null` when `value` is `null`, and also
   * when a `projectId` names a project `projects` has not caught up to (or
   * that this device has never seen). Turning `null` into display text is the
   * caller's job, so this module stays a pure data shape. */
  label: string | null;
  items: TaskItemDTO[];
}

function rawAxisValue(item: TaskItemDTO, axis: FrontierAxis): string | null {
  if (axis === "context") {
    return item.context;
  }
  if (axis === "project") {
    return item.projectId;
  }
  if (axis === "size") {
    return item.size;
  }
  return item.energy;
}

/** An **empty string is not a value**, and is folded into the no-value bucket.
 *
 * This is not defensive noise. `items.context` is free text —
 * `context TEXT` with no CHECK (`server/authority/src/schema.rs`) and no
 * empty-string rejection on the write path — so while every in-app writer
 * normalises `""` to `null` (`screens/triage-form.ts`, `screens/capture-meta.ts`),
 * any API writer holding a `device` token can land a `""`. Without this fold it
 * would produce a *second* no-value column: the caller keys columns by their
 * value with `null` rendered as `""`, so the two would share a React key, share
 * one collapse entry, and one of them would render a heading with no accessible
 * name at all. Keeping the no-value bucket single is what keeps that key
 * unforgeable. */
function axisValue(item: TaskItemDTO, axis: FrontierAxis): string | null {
  const raw = rawAxisValue(item, axis);
  return raw === null || raw === "" ? null : raw;
}

/** Columns for one axis: **fullest first**, and the "names no value on this
 * axis" column **always last** whichever axis is live.
 *
 * Fullest-first is the board's own answer to "where is the work". Last-place
 * for the unnamed bucket generalises the rule `frontier-groups.ts` already
 * stated for one axis — "an unassigned item should never visually outrank a
 * real project's section" — which is now needed four times over.
 *
 * Ties are broken by first appearance in `items`, because `Array#sort` is
 * stable: two columns of equal height keep the order the input gave them, so
 * the same input always yields the same output rather than an arbitrary one. */
export function groupFrontier(
  items: readonly TaskItemDTO[],
  axis: FrontierAxis,
  projects: readonly ProjectDTO[],
): FrontierColumn[] {
  const namesById = new Map(projects.map((project) => [project.id, project.name]));

  const order: (string | null)[] = [];
  const byValue = new Map<string | null, TaskItemDTO[]>();

  for (const item of items) {
    const value = axisValue(item, axis);
    const bucket = byValue.get(value);
    if (bucket) {
      bucket.push(item);
    } else {
      byValue.set(value, [item]);
      order.push(value);
    }
  }

  const columns = order.map((value) => ({
    value,
    label: value === null ? null : axis === "project" ? (namesById.get(value) ?? null) : value,
    items: byValue.get(value) ?? [],
  }));

  const named = columns.filter((column) => column.value !== null);
  const unnamed = columns.filter((column) => column.value === null);

  named.sort((a, b) => b.items.length - a.items.length);

  return [...named, ...unnamed];
}
