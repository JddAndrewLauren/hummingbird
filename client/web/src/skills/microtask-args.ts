// The `POST /api/skills/run` body for one microtask run (#273).

export interface MicrotaskRunInput {
  /** The item's uuid. */
  itemId: string;
  /** Present-and-true only, on the rewrite gesture. */
  replace?: boolean;
  /** SKILL.md's 1-3 grain scale. Omitted leaves the runner's default (2). */
  grain?: number;
  /** Omitted leaves the runner's configured model. */
  model?: string;
}

export interface MicrotaskRunBody {
  skill: "microtask";
  args: Record<string, unknown>;
}

/**
 * Three rules, each of which is a bug if it goes the other way:
 *
 * - **`ref` is the uuid, never `HB-<seq>`.** The runner accepts both, but
 *   `seq` is nullable on `TaskItemDTO` and a locally-minted item that has
 *   not synced yet has none — a `HB-null` would resolve to nothing.
 * - **`replace` is sent present-and-`true` or not at all.** A literal
 *   `replace: false` is a valid boolean the runner would accept, and it
 *   says the same thing as omitting it while making a bare run look like a
 *   decision about rewriting.
 * - **`grain` and `model` are omitted when unset**, so the runner's
 *   defaults stay the defaults rather than being re-specified here, where
 *   they would drift.
 */
export function microtaskRunBody(input: MicrotaskRunInput): MicrotaskRunBody {
  const args: Record<string, unknown> = { ref: input.itemId };
  if (input.replace === true) args.replace = true;
  if (input.grain !== undefined) args.grain = input.grain;
  if (input.model !== undefined && input.model !== "") args.model = input.model;
  return { skill: "microtask", args };
}
