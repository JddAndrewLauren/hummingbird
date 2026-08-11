// #208's pure half: turns the capture box's `Slider`/`Select` state into
// the wire's `CaptureFields` shape (`store/worker-client.ts`), the same
// split `capture-validation.ts`/`triage-form.ts` already use for their own
// screen logic — no React, no worker, unit-testable on its own.

import type { CaptureFields } from "../store/worker-client";

/** The capture box's local Energy/Size/Context state
 * (`screens/CaptureBox.tsx`). `energy`/`size` are `Slider` stop indices
 * (`null` = not set, the resting state); `context` is the `Select`'s own
 * value (`""` = not set). */
export interface CaptureMeta {
  energy: number | null;
  size: number | null;
  context: string;
}

export const EMPTY_CAPTURE_META: CaptureMeta = { energy: null, size: null, context: "" };

/** `Slider` index -> `hummingbird_domain::Size`'s own wire name, in the
 * slider's own left-to-right stop order (`CaptureBox.tsx`'s
 * `CAPTURE_SIZE_STOPS` — "normal" is the slider's
 * display label; the domain vocabulary at that stop is `"short"`).
 *
 * Indexed by the raw slider index, and hand-aligned with
 * `CaptureBox.tsx`'s `CAPTURE_SIZE_STOPS` — nothing mechanical connects
 * the two, so `capture-meta.test.ts` asserts their lengths agree. An index
 * past the end resolves to `undefined`, which reads downstream as "not set":
 * a silently dropped selection, never an error. Exported for that test
 * only. */
export const CAPTURE_SIZE_NAMES: ReadonlyArray<"quick" | "short" | "deep"> = [
  "quick",
  "short",
  "deep",
];

/** `Slider` index -> `hummingbird_domain::Energy`'s own wire name — the
 * slider's display labels already match the domain vocabulary here. Same
 * hand-alignment hazard as `CAPTURE_SIZE_NAMES` above, against
 * `CaptureBox.tsx`'s `CAPTURE_ENERGY_STOPS`. */
export const CAPTURE_ENERGY_NAMES: ReadonlyArray<"low" | "medium" | "high"> = [
  "low",
  "medium",
  "high",
];

/** Resolves one capture box's `CaptureMeta` to the wire's `CaptureFields`:
 * an unset slider or an empty context select maps to `null` ("not set"),
 * never a default index or an empty string reaching the mutation — the
 * "optional, decided at mint time" contract this issue must not break. */
export function resolveCaptureFields(meta: CaptureMeta): CaptureFields {
  return {
    size: meta.size === null ? null : CAPTURE_SIZE_NAMES[meta.size],
    energy: meta.energy === null ? null : CAPTURE_ENERGY_NAMES[meta.energy],
    context: meta.context === "" ? null : meta.context,
  };
}
