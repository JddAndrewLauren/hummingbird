// The web side of the shared run-body fixture (#538).
//
// `client/core/tests/fixtures/skills-run-bodies.json` is read by three
// languages — Rust (`client/core/tests/skills_run_bodies.rs`), this file,
// and the Android instrumented suite — each asserting its own builder emits
// `expected` byte for byte. Reading the real committed file across a
// language boundary is `race.test.ts`'s move with
// `server/race-poll/tests/fixtures/golden-body.json`; a fixture retyped
// here would agree with itself forever while the two sides drifted.
//
// **This asserts the real wire text.** `run-skill.ts` posts exactly
// `JSON.stringify(body)`, so re-stringifying the object these builders
// return is the same bytes the runner receives — key order included, which
// is the part a shape assertion could never catch.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { grillRunBody, type GrillTurn } from "./grill-args";
import { microtaskRunBody, type MicrotaskRunInput } from "./microtask-args";

interface Case {
  name: string;
  skill: "microtask" | "grill-me";
  input: unknown;
  expected: string;
}

const FIXTURE = JSON.parse(
  readFileSync(
    new URL("../../../core/tests/fixtures/skills-run-bodies.json", import.meta.url),
    "utf8",
  ),
) as { cases: Case[] };

describe("the shared run-body fixture", () => {
  it("carries cases (a fixture that lost its list would pass vacuously below)", () => {
    expect(FIXTURE.cases.length).toBeGreaterThan(0);
    expect(FIXTURE.cases.map((one) => one.skill)).toContain("microtask");
    expect(FIXTURE.cases.map((one) => one.skill)).toContain("grill-me");
  });

  it.each(FIXTURE.cases.map((one) => [one.name, one] as const))("%s", (_name, one) => {
    const body =
      one.skill === "microtask"
        ? microtaskRunBody(one.input as MicrotaskRunInput)
        : grillRunBody(one.input as { ref: string; turns: GrillTurn[] });
    expect(JSON.stringify(body)).toBe(one.expected);
  });
});
