import { describe, expect, it } from "vitest";
import { buildTriageEdits, EMPTY_TRIAGE_DRAFT } from "./triage-form";

describe("buildTriageEdits", () => {
  it("sends nothing but nulls for an untouched draft", () => {
    expect(buildTriageEdits(EMPTY_TRIAGE_DRAFT, "someday maybe")).toEqual({
      title: null,
      projectId: null,
      size: null,
      energy: null,
      context: null,
    });
  });

  it("carries a title only when it actually changed", () => {
    expect(buildTriageEdits({ ...EMPTY_TRIAGE_DRAFT, title: "someday maybe" }, "someday maybe")).toEqual(
      expect.objectContaining({ title: null }),
    );
    expect(buildTriageEdits({ ...EMPTY_TRIAGE_DRAFT, title: "buy milk" }, "someday maybe")).toEqual(
      expect.objectContaining({ title: "buy milk" }),
    );
  });

  it("trims a title before comparing and sending it", () => {
    expect(buildTriageEdits({ ...EMPTY_TRIAGE_DRAFT, title: "  buy milk  " }, "someday maybe")).toEqual(
      expect.objectContaining({ title: "buy milk" }),
    );
  });

  it("never sends a whitespace-only title as a real edit", () => {
    expect(buildTriageEdits({ ...EMPTY_TRIAGE_DRAFT, title: "   " }, "someday maybe")).toEqual(
      expect.objectContaining({ title: null }),
    );
  });

  it("carries every set field, resolved by its own vocabulary name, in one call", () => {
    expect(
      buildTriageEdits(
        {
          title: "buy milk",
          projectId: "project-1",
          size: "quick",
          energy: "low",
          context: "@errands",
        },
        "someday maybe",
      ),
    ).toEqual({
      title: "buy milk",
      projectId: "project-1",
      size: "quick",
      energy: "low",
      context: "@errands",
    });
  });
});
