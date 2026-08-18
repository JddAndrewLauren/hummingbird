// The `POST /api/skills/run` body for one microtask run (#273).
//
// **The rules are `hummingbird_core::decisions::skills::args`'s** since
// #538 sank them there (ADR-0025). The core answers with the canonical
// request *text*, byte-pinned across Rust, TypeScript and Kotlin by
// `client/core/tests/fixtures/skills-run-bodies.json` — and since
// `run-skill.ts` posts exactly `JSON.stringify(body)`, parsing that text
// back into the object shape below preserves its key order, which is what
// makes the fixture a pin on the real wire bytes rather than on a shape.

import { microtaskRunBodyJson } from "../decisions/seam";

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
 * Three rules, each of which is a bug if it goes the other way — all three
 * now stated once, in the core:
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
  return JSON.parse(microtaskRunBodyJson(input)) as MicrotaskRunBody;
}
