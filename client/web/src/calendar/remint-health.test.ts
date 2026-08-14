import { describe, expect, it } from "vitest";
import { TOKEN_TIMEOUT_ERROR } from "../google/gis";
import {
  blockingRemintErrors,
  INITIAL_REMINT_HEALTH,
  isNonBlockingRemintError,
  recordSilentRemint,
  SILENT_REMINT_FAILURE_LIMIT,
} from "./remint-health";

const OFFLINE_ERRORS = ["gis_script_load_failed", "gis_unavailable", "gis_request_failed"];

function fold(errors: (string | null)[]) {
  return errors.reduce(recordSilentRemint, INITIAL_REMINT_HEALTH);
}

describe("recordSilentRemint", () => {
  it("does not block on one failure", () => {
    // A single failure is routinely transient — a session being refreshed, a
    // request racing a sign-in elsewhere. Blocking on it would turn one
    // hiccup into a manual reconnect.
    expect(fold(["interaction_required"])).toEqual({ consecutiveFailures: 1, blocked: false });
  });

  it("blocks at the limit", () => {
    const health = fold(Array<string>(SILENT_REMINT_FAILURE_LIMIT).fill("login_required"));
    expect(health.blocked).toBe(true);
  });

  it("any success resets completely", () => {
    const recovered = fold(["interaction_required", "login_required", null]);
    expect(recovered).toEqual(INITIAL_REMINT_HEALTH);
    // And the count really is back to zero, not merely unblocked: one further
    // failure must not re-block immediately.
    expect(recordSilentRemint(recovered, "interaction_required").blocked).toBe(false);
  });

  it.each(blockingRemintErrors())("counts %s toward blocking", (error) => {
    expect(fold([error, error]).blocked).toBe(true);
  });

  // The one that matters most. Blocking on these would push a merely-offline
  // reader into an interactive consent screen they cannot complete anyway.
  it.each(OFFLINE_ERRORS)("never blocks on %s, however many times it happens", (error) => {
    expect(fold(Array<string>(20).fill(error))).toEqual(INITIAL_REMINT_HEALTH);
  });

  it("an offline failure neither counts nor forgives a real run", () => {
    // It is not evidence the session recovered, so the run it interrupts
    // stands — one blocking failure either side still blocks.
    expect(fold(["interaction_required", "gis_script_load_failed", "login_required"]).blocked).toBe(
      true,
    );
  });

  it("a timed-out silent re-mint is the ITP signature, and counts", () => {
    // The iframe loaded and never posted back — exactly what a partitioned
    // cookie jar looks like from this side. Retrying costs 15s and can never
    // succeed.
    expect(fold([TOKEN_TIMEOUT_ERROR, TOKEN_TIMEOUT_ERROR]).blocked).toBe(true);
  });

  it("classifies every error into exactly one of the two sets", () => {
    for (const error of blockingRemintErrors()) {
      expect(isNonBlockingRemintError(error)).toBe(false);
    }
    for (const error of OFFLINE_ERRORS) {
      expect(isNonBlockingRemintError(error)).toBe(true);
      expect(blockingRemintErrors()).not.toContain(error);
    }
  });

  it("an unrecognised code does not block", () => {
    // Default-deny, the product's own habit: a code nobody has classified is
    // not evidence a human is required, and guessing wrong here nags the
    // reader every hour.
    expect(fold(Array<string>(5).fill("something_new"))).toEqual(INITIAL_REMINT_HEALTH);
  });
});
