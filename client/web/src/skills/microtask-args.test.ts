import { describe, expect, it } from "vitest";
import { microtaskRunBody } from "./microtask-args";

describe("microtaskRunBody", () => {
  /** `seq` is nullable on `TaskItemDTO` — a locally-minted item that has not
   * synced yet has none, and `HB-null` would resolve to nothing. */
  it("ref is the uuid, never HB-<seq>", () => {
    expect(microtaskRunBody({ itemId: "8f2c-uuid" })).toEqual({
      skill: "microtask",
      args: { ref: "8f2c-uuid" },
    });
  });

  it("a bare run sends nothing but the ref", () => {
    const body = microtaskRunBody({ itemId: "i" });
    expect(Object.keys(body.args)).toEqual(["ref"]);
  });

  /** A literal `replace: false` is a valid boolean the runner accepts, and
   * it says the same thing as omitting it while making a bare run look like
   * a decision about rewriting. */
  it("replace is present-and-true or absent, never false", () => {
    expect(microtaskRunBody({ itemId: "i", replace: true }).args.replace).toBe(true);
    expect("replace" in microtaskRunBody({ itemId: "i", replace: false }).args).toBe(false);
    expect("replace" in microtaskRunBody({ itemId: "i" }).args).toBe(false);
  });

  it("grain and model ride along when set", () => {
    expect(microtaskRunBody({ itemId: "i", replace: true, grain: 3, model: "opus" })).toEqual({
      skill: "microtask",
      args: { ref: "i", replace: true, grain: 3, model: "opus" },
    });
  });

  /** The empty value is the "Default model" option — omitting the key is
   * what leaves the runner's own default in place rather than naming it
   * twice, here and there. */
  it("an empty model omits the key entirely", () => {
    expect("model" in microtaskRunBody({ itemId: "i", model: "" }).args).toBe(false);
  });
});
