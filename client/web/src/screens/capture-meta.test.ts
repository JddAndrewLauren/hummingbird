import { describe, expect, it } from "vitest";
import {
  CAPTURE_ENERGY_NAMES,
  CAPTURE_SIZE_NAMES,
  EMPTY_CAPTURE_META,
  resolveCaptureFields,
} from "./capture-meta";
import { CAPTURE_ENERGY_STOPS, CAPTURE_SIZE_STOPS } from "./TriageScreen";

describe("resolveCaptureFields", () => {
  it("leaves all three absent when the meta is at its resting state", () => {
    expect(resolveCaptureFields(EMPTY_CAPTURE_META)).toEqual({
      size: null,
      energy: null,
      context: null,
    });
  });

  it("resolves every size slider stop to the domain's own wire name", () => {
    expect(resolveCaptureFields({ energy: null, size: 0, context: "" }).size).toBe("quick");
    expect(resolveCaptureFields({ energy: null, size: 1, context: "" }).size).toBe("short");
    expect(resolveCaptureFields({ energy: null, size: 2, context: "" }).size).toBe("deep");
  });

  it("resolves every energy slider stop to the domain's own wire name", () => {
    expect(resolveCaptureFields({ energy: 0, size: null, context: "" }).energy).toBe("low");
    expect(resolveCaptureFields({ energy: 1, size: null, context: "" }).energy).toBe("medium");
    expect(resolveCaptureFields({ energy: 2, size: null, context: "" }).energy).toBe("high");
  });

  it("carries a chosen context through, and maps an empty one to null", () => {
    expect(resolveCaptureFields({ energy: null, size: null, context: "@home" }).context).toBe(
      "@home",
    );
    expect(resolveCaptureFields({ energy: null, size: null, context: "" }).context).toBeNull();
  });

  it("sets only the one field a caller touched, leaving the other two absent", () => {
    expect(resolveCaptureFields({ energy: 2, size: null, context: "" })).toEqual({
      size: null,
      energy: "high",
      context: null,
    });
  });

  // Pinning the CURRENT behaviour, not endorsing it. An index past the end
  // of the name array is not `null` ("the reader left this at rest") — it is
  // `undefined`, which every downstream `?? null` quietly converts INTO
  // "not set". So the failure mode of adding a fourth slider stop without a
  // fourth name is a selection that vanishes with no error anywhere: the
  // capture succeeds, and the field the reader chose is simply gone. This
  // test exists so that behaviour is at least written down; the test below
  // is the one that stops it happening.
  it("drops an out-of-range slider index to undefined rather than erroring", () => {
    const fields = resolveCaptureFields({ energy: 3, size: 3, context: "" });
    expect(fields.size).toBeUndefined();
    expect(fields.energy).toBeUndefined();
    // And that is indistinguishable from "not set" one `?? null` later.
    expect(fields.size ?? null).toBeNull();
    expect(fields.energy ?? null).toBeNull();
  });

  it("survives a negative index the same way, without throwing", () => {
    expect(resolveCaptureFields({ energy: -1, size: -1, context: "" }).size).toBeUndefined();
  });
});

// The missing mechanism between the two hand-aligned sides: `capture-meta.ts`
// indexes `CAPTURE_SIZE_NAMES`/`CAPTURE_ENERGY_NAMES` by the raw index the
// `Slider` in `TriageScreen.tsx` produces, and the compiler cannot see that
// the arrays are supposed to correspond. A fourth stop added to one side and
// not the other used to be a silently dropped selection (the test above);
// now it is a red test.
describe("the capture sliders and their wire-name arrays", () => {
  it("offers exactly as many size stops as there are size names", () => {
    expect(CAPTURE_SIZE_STOPS.length).toBe(CAPTURE_SIZE_NAMES.length);
  });

  it("offers exactly as many energy stops as there are energy names", () => {
    expect(CAPTURE_ENERGY_STOPS.length).toBe(CAPTURE_ENERGY_NAMES.length);
  });

  // A `Slider` with fewer than two stops renders "nothing to choose from"
  // instead of a track (see `Slider.tsx`), so an emptied array would take
  // the control off the screen rather than fail an index lookup.
  it("keeps both sliders at two stops or more", () => {
    expect(CAPTURE_SIZE_STOPS.length).toBeGreaterThanOrEqual(2);
    expect(CAPTURE_ENERGY_STOPS.length).toBeGreaterThanOrEqual(2);
  });
});
