// #208's pure half: turns the capture box's `Slider`/`Select` state into
// the wire's `CaptureFields` shape (`store/worker-client.ts`), the same
// split `capture-validation.ts`/`triage-form.ts` already use for their own
// screen logic — no React, no worker, unit-testable on its own.

import type { CaptureFields } from "../store/worker-client";

/** The capture box's local Energy/Size/Context state
 * (`screens/TriageScreen.tsx`). `energy`/`size` are `Slider` stop indices
 * (`null` = not set, the resting state); `context` is the `Select`'s own
 * value (`""` = not set). */
export interface CaptureMeta {
  energy: number | null;
  size: number | null;
  context: string;
}

export const EMPTY_CAPTURE_META: CaptureMeta = { energy: null, size: null, context: "" };

/** `Slider` index -> `hummingbird_domain::Size`'s own wire name, in the
 * slider's own left-to-right stop order (`TriageScreen.tsx`'s
 * `options={["quick", "normal", "deep"]}` — "normal" is the slider's
 * display label; the domain vocabulary at that stop is `"short"`). */
const SIZE_NAMES: ReadonlyArray<"quick" | "short" | "deep"> = ["quick", "short", "deep"];

/** `Slider` index -> `hummingbird_domain::Energy`'s own wire name — the
 * slider's display labels already match the domain vocabulary here. */
const ENERGY_NAMES: ReadonlyArray<"low" | "medium" | "high"> = ["low", "medium", "high"];

/** Resolves one capture box's `CaptureMeta` to the wire's `CaptureFields`:
 * an unset slider or an empty context select maps to `null` ("not set"),
 * never a default index or an empty string reaching the mutation — the
 * "optional, decided at mint time" contract this issue must not break. */
export function resolveCaptureFields(meta: CaptureMeta): CaptureFields {
  return {
    size: meta.size === null ? null : SIZE_NAMES[meta.size],
    energy: meta.energy === null ? null : ENERGY_NAMES[meta.energy],
    context: meta.context === "" ? null : meta.context,
  };
}
