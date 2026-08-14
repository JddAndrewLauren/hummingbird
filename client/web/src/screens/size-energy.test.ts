import { describe, expect, it } from "vitest";
import {
  energyIcon,
  energyLabel,
  energyTitle,
  levelColor,
  sizeIcon,
  sizeLabel,
  sizeTitle,
} from "./size-energy";

describe("sizeIcon / energyIcon", () => {
  it("names one glyph per level", () => {
    expect(sizeIcon("quick")).toBe("size-quick");
    expect(sizeIcon("normal")).toBe("size-normal");
    expect(sizeIcon("deep")).toBe("size-deep");
    expect(energyIcon("low")).toBe("energy-low");
    expect(energyIcon("medium")).toBe("energy-medium");
    expect(energyIcon("high")).toBe("energy-high");
  });

  // The state a green build and a screenshot both miss: unset is where five
  // call sites hand this module a `null`, and it must produce a glyph rather
  // than a blank box or an escalation.
  it("gives an unjudged dimension its own ghost glyph, not nothing", () => {
    expect(sizeIcon(null)).toBe("size-unset");
    expect(energyIcon(null)).toBe("energy-unset");
  });
});

describe("levelColor", () => {
  // Position on the scale, not name: this is the claim that lets one ramp
  // serve both dimensions, so the two must agree step for step.
  it("gives the same colour to the same position on either scale", () => {
    expect(levelColor("quick")).toBe(levelColor("low"));
    expect(levelColor("normal")).toBe(levelColor("medium"));
    expect(levelColor("deep")).toBe(levelColor("high"));
  });

  it("borrows the urgency tokens for the top two steps", () => {
    expect(levelColor("quick")).toBe("var(--status-done-fg)");
    expect(levelColor("normal")).toBe("var(--urgency-soon)");
    expect(levelColor("deep")).toBe("var(--urgency-now)");
  });

  it("keeps unset muted — a resting state, never a warning", () => {
    expect(levelColor(null)).toBe("var(--text-muted)");
    expect(levelColor(null)).not.toBe(levelColor("deep"));
  });

  it("gives every step its own colour, so the ramp is readable as a ramp", () => {
    const steps = [levelColor(null), levelColor("quick"), levelColor("normal"), levelColor("deep")];
    expect(new Set(steps).size).toBe(4);
  });
});

describe("sizeLabel / energyLabel", () => {
  it("uppercases the wire word for the mono meta treatment", () => {
    expect(sizeLabel("normal")).toBe("NORMAL");
    expect(energyLabel("high")).toBe("HIGH");
  });

  it("renders an em dash for an unjudged dimension", () => {
    expect(sizeLabel(null)).toBe("—");
    expect(energyLabel(null)).toBe("—");
  });
});

// The accessible name for the four surfaces that draw the glyph with no word
// beside it (ADR-0024). Dropping the label is only defensible because this
// exists, so it is asserted rather than trusted.
describe("sizeTitle / energyTitle", () => {
  it("names the dimension and its level, in sentence case", () => {
    expect(sizeTitle("normal")).toBe("Size: normal");
    expect(energyTitle("high")).toBe("Energy: high");
  });

  // Not the label's uppercase: this is read aloud or shown as a tooltip.
  it("does not shout, unlike the visible label", () => {
    expect(sizeTitle("deep")).not.toBe(`Size: ${sizeLabel("deep")}`);
  });

  it("stays total, so an unjudged dimension still names itself", () => {
    expect(sizeTitle(null)).toBe("Size: not judged");
    expect(energyTitle(null)).toBe("Energy: not judged");
  });
});
