import { describe, expect, it } from "vitest";
import type { Band, PaneAnswer } from "../questions/contract";
import { bandWord, subjectCount, tileParts, tileTone } from "./tile-copy";

function answer(overrides: Partial<PaneAnswer> = {}): PaneAnswer {
  return {
    answerState: "answered",
    band: "dormant",
    withinBand: null,
    collapsedHeadline: "authority · 401 as expected",
    ...overrides,
  };
}

describe("the Status board's compact copy", () => {
  it("splits a pane's sentence into the subject and what happened", () => {
    expect(tileParts("Uptime", "authority · 401 as expected")).toEqual({
      name: "authority",
      fact: "401 as expected",
    });
  });

  // An uptime transport error can carry its own separator, and the subject is
  // always the head — so a later one must not move the split.
  it("splits on the first separator only, never a later one inside the fact", () => {
    expect(tileParts("Uptime", "runner · unreachable — a · b")).toEqual({
      name: "runner",
      fact: "unreachable — a · b",
    });
  });

  it("keeps the question's own label when the sentence has no subject in it", () => {
    expect(tileParts("This device", "Synced 12m ago")).toEqual({
      name: "This device",
      fact: "Synced 12m ago",
    });
    expect(tileParts("Kimi balance", "Never synced on this device.")).toEqual({
      name: "Kimi balance",
      fact: "Never synced on this device.",
    });
  });

  // Reachable from real data: an empty `display_name` in a workflow payload,
  // or a truncated one, puts the separator at the head.
  it("keeps the label when the sentence starts with the separator", () => {
    expect(tileParts("GitHub workflows", " · never run")).toEqual({
      name: "GitHub workflows",
      fact: "never run",
    });
  });

  it("says something when the sentence is nothing but a separator", () => {
    const { name, fact } = tileParts("Uptime", " · ");
    expect(name).toBe("Uptime");
    expect(fact.trim()).not.toBe("");
  });

  it("rewrites no words at all", () => {
    const headline =
      "gmail-poll · cadence unreadable, last scheduled success 6h ago";
    const { name, fact } = tileParts("GitHub workflows", headline);
    expect(`${name} · ${fact}`).toBe(headline);
  });

  it("reads the two pressing bands as danger and the two middling ones as warn", () => {
    const tones: Record<Band, string> = {
      live: "danger",
      imminent: "danger",
      near: "warn",
      distant: "warn",
      dormant: "quiet",
    };
    for (const [band, tone] of Object.entries(tones)) {
      expect(tileTone(answer({ band: band as Band }))).toBe(tone);
    }
  });

  // The distinction the board must not flatten: a gap has no answer to call
  // expected, so it is never quiet — whatever band it happens to carry.
  it("reads any unanswered pane as a gap, never as quiet", () => {
    expect(
      tileTone(
        answer({ answerState: "bound-but-unacquired", band: "dormant" }),
      ),
    ).toBe("gap");
    expect(tileTone(answer({ answerState: "unbound", band: "dormant" }))).toBe(
      "gap",
    );
  });

  it("says the band in the band vocabulary's own words", () => {
    expect(bandWord("live")).toBe("band:live");
    expect(bandWord("dormant")).toBe("band:dormant");
  });

  it("counts subjects, singular and plural", () => {
    expect(subjectCount(1)).toBe("1 subject");
    expect(subjectCount(5)).toBe("5 subjects");
  });
});
