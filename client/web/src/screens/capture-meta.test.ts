import { describe, expect, it } from "vitest";
import {
  CAPTURE_ENERGY_NAMES,
  CAPTURE_SIZE_NAMES,
  EMPTY_CAPTURE_META,
  captureMetaProblems,
  resolveCaptureFields,
  todayDeadline,
  type CaptureMeta,
} from "./capture-meta";

/** The resting state with one or two fields moved off it — every field is
 * optional on this form, so most cases are "exactly one thing chosen". */
function meta(overrides: Partial<CaptureMeta>): CaptureMeta {
  return { ...EMPTY_CAPTURE_META, ...overrides };
}

describe("resolveCaptureFields", () => {
  it("leaves every field absent when the meta is at its resting state", () => {
    expect(resolveCaptureFields(EMPTY_CAPTURE_META)).toEqual({
      size: null,
      energy: null,
      context: null,
      description: null,
      projectId: null,
      priority: null,
      deadline: null,
      scheduledDate: null,
    });
  });

  it("resolves the typed fields, trimming a description and dropping an empty one", () => {
    const fields = resolveCaptureFields(
      meta({
        description: "  the oat kind  ",
        projectId: "proj-1",
        deadline: "2026-09-01T09:30",
        scheduledDate: "2026-08-30",
      }),
    );
    expect(fields.description).toBe("the oat kind");
    expect(fields.projectId).toBe("proj-1");
    expect(fields.deadline).toBe("2026-09-01T09:30");
    expect(fields.scheduledDate).toBe("2026-08-30");
    expect(resolveCaptureFields(meta({ description: "   " })).description).toBeNull();
  });

  // The priority vocabulary's own "none" is 0, and the server defaults to 0.
  // Sending it would make priority the one capture field that could not be
  // left undecided — a value the reader never chose, asserted on their behalf.
  it("treats the resting priority as not-sent, and sends every other one as a number", () => {
    expect(resolveCaptureFields(meta({ priority: "0" })).priority).toBeNull();
    expect(resolveCaptureFields(meta({ priority: "1" })).priority).toBe(1);
    expect(resolveCaptureFields(meta({ priority: "4" })).priority).toBe(4);
  });

  it("resolves every size slider stop to the domain's own wire name", () => {
    // `meta()` is this branch's helper — the draft grew fields past the three
    // scalars main's literal spells out. The middle value is ADR-0024's
    // `normal`, which is what main renamed it to.
    expect(resolveCaptureFields(meta({ size: 0 })).size).toBe("quick");
    expect(resolveCaptureFields(meta({ size: 1 })).size).toBe("normal");
    expect(resolveCaptureFields(meta({ size: 2 })).size).toBe("deep");
  });

  it("resolves every energy slider stop to the domain's own wire name", () => {
    expect(resolveCaptureFields(meta({ energy: 0 })).energy).toBe("low");
    expect(resolveCaptureFields(meta({ energy: 1 })).energy).toBe("medium");
    expect(resolveCaptureFields(meta({ energy: 2 })).energy).toBe("high");
  });

  it("carries a chosen context through, and maps an empty one to null", () => {
    expect(resolveCaptureFields(meta({ context: "@home" })).context).toBe(
      "@home",
    );
    expect(resolveCaptureFields(meta({})).context).toBeNull();
  });

  it("sets only the one field a caller touched, leaving every other absent", () => {
    expect(resolveCaptureFields(meta({ energy: 2 }))).toEqual({
      size: null,
      energy: "high",
      context: null,
      description: null,
      projectId: null,
      priority: null,
      deadline: null,
      scheduledDate: null,
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
    const fields = resolveCaptureFields(meta({ energy: 3, size: 3 }));
    expect(fields.size).toBeUndefined();
    expect(fields.energy).toBeUndefined();
    // And that is indistinguishable from "not set" one `?? null` later.
    expect(fields.size ?? null).toBeNull();
    expect(fields.energy ?? null).toBeNull();
  });

  it("survives a negative index the same way, without throwing", () => {
    expect(resolveCaptureFields(meta({ energy: -1, size: -1 })).size).toBeUndefined();
  });
});

// The same rules `triage-form.ts` applies to the same values, with the same
// messages: one form must not accept what the other rejects, and the seam
// refuses both identically (`is_valid_deadline`).
describe("captureMetaProblems", () => {
  it("finds nothing wrong with the resting state — every field is optional", () => {
    expect(captureMetaProblems(EMPTY_CAPTURE_META)).toEqual({});
  });

  it("accepts both deadline shapes and a whole-day scheduled date", () => {
    expect(captureMetaProblems(meta({ deadline: "2026-09-01" }))).toEqual({});
    expect(captureMetaProblems(meta({ deadline: "2026-09-01T09:30" }))).toEqual({});
    expect(captureMetaProblems(meta({ scheduledDate: "2026-08-30" }))).toEqual({});
  });

  it("names a malformed deadline, an impossible date, and a timed do-date", () => {
    expect(captureMetaProblems(meta({ deadline: "next tuesday" })).deadline).toBe(
      "Use YYYY-MM-DD or YYYY-MM-DDTHH:MM",
    );
    expect(captureMetaProblems(meta({ deadline: "2026-02-30" })).deadline).toBeDefined();
    // A do-date is a whole civil day; the date-time form is refused here even
    // though the deadline field accepts it.
    expect(captureMetaProblems(meta({ scheduledDate: "2026-08-30T09:30" })).scheduledDate).toBe(
      "Use YYYY-MM-DD",
    );
  });
});

// One array, so there is no longer a pair that can disagree — the length
// assertion that used to live here went with the second array (see
// `capture-meta.ts`). What survives is the one claim that is still about a
// single array: a `Slider` with fewer than two stops renders "nothing to
// choose from" instead of a track (`Slider.tsx`), so an emptied array would
// take the control off the screen rather than fail an index lookup.
describe("the capture sliders' stops", () => {
  it("keeps both sliders at two stops or more", () => {
    expect(CAPTURE_SIZE_NAMES.length).toBeGreaterThanOrEqual(2);
    expect(CAPTURE_ENERGY_NAMES.length).toBeGreaterThanOrEqual(2);
  });
});

describe("todayDeadline", () => {
  it("renders the local calendar date in the wire's whole-day form", () => {
    expect(todayDeadline(new Date(2026, 8, 3, 14, 30).getTime())).toBe("2026-09-03");
  });

  it("reads the local day, not the UTC one, either side of midnight", () => {
    // 23:30 local on the 3rd stays the 3rd whatever UTC calls it; 00:30 on
    // the 4th is the 4th. Built with the local-time constructor, so the
    // expectation holds in every zone the suite runs in.
    expect(todayDeadline(new Date(2026, 8, 3, 23, 30).getTime())).toBe("2026-09-03");
    expect(todayDeadline(new Date(2026, 8, 4, 0, 30).getTime())).toBe("2026-09-04");
  });
});
