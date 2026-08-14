import { describe, expect, it } from "vitest";
import { TOKEN_TIMEOUT_ERROR } from "../google/gis";
import {
  blockingRemintErrors,
  classifyRemintError,
  INITIAL_REMINT_HEALTH,
  nonBlockingRemintErrors,
  recordInteractiveConnect,
  recordSilentRemint,
  SILENT_REMINT_FAILURE_LIMIT,
} from "./remint-health";

const OFFLINE_ERRORS = ["gis_script_load_failed", "gis_unavailable", "gis_request_failed"];

/** Every code a `prompt: "none"` request can actually come back with — read
 * off `google/gis.ts`'s settle sites, plus the `error_callback` types Google
 * documents for a silent request. This is the list the classification has to
 * be TOTAL over: a code here that nobody has classified is the accident the
 * two sets exist to prevent, and it is not caught by asserting the sets are
 * disjoint. Interactive-only codes (`popup_closed`, `popup_failed_to_open`)
 * and the redirect flow's own (`state_mismatch`, `no_expiry`) are deliberately
 * absent — see `classifyRemintError`'s doc. */
const SILENT_PATH_ERRORS = [
  "interaction_required",
  "login_required",
  "consent_required",
  "access_denied",
  "user_logged_out",
  TOKEN_TIMEOUT_ERROR,
  "no_access_token",
  ...OFFLINE_ERRORS,
];

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

  it("retries nothing whose documented remedy is a human, no_access_token included", () => {
    // `connect-error.ts` tells this reader to remove the app's calendar access
    // in their Google account and connect again. It was in neither set, so the
    // reducer's `!BLOCKING_ERRORS.has(error)` branch retried it hourly forever
    // — the exact nag this module exists to stop.
    expect(classifyRemintError("no_access_token")).toBe("blocking");
    expect(fold(["no_access_token", "no_access_token"]).blocked).toBe(true);
  });

  it("classifies every error the silent path can produce, and none twice", () => {
    for (const error of SILENT_PATH_ERRORS) {
      // Totality: `unclassified` here means a code reached the reducer and got
      // its behaviour by falling off the end of both sets rather than by a
      // decision. That is what this file used to allow.
      expect(classifyRemintError(error)).not.toBe("unclassified");
    }
    for (const error of blockingRemintErrors()) {
      expect(classifyRemintError(error)).toBe("blocking");
      expect(nonBlockingRemintErrors()).not.toContain(error);
    }
    for (const error of nonBlockingRemintErrors()) {
      expect(classifyRemintError(error)).toBe("non-blocking");
      expect(blockingRemintErrors()).not.toContain(error);
    }
  });

  it("makes the reducer agree with the classification, code by code", () => {
    // The classification is only worth anything if it is what the counter
    // actually consults. Driven off the sets themselves, so a code added to
    // one of them without the reducer honouring it fails here.
    for (const error of SILENT_PATH_ERRORS) {
      const blocks = classifyRemintError(error) === "blocking";
      expect(fold(Array<string>(SILENT_REMINT_FAILURE_LIMIT).fill(error)).blocked).toBe(blocks);
    }
  });

  it("an unrecognised code does not block", () => {
    // Default-deny, the product's own habit: a code nobody has classified is
    // not evidence a human is required, and guessing wrong here nags the
    // reader every hour.
    expect(classifyRemintError("something_new")).toBe("unclassified");
    expect(fold(Array<string>(5).fill("something_new"))).toEqual(INITIAL_REMINT_HEALTH);
  });
});

describe("recordInteractiveConnect", () => {
  it("lifts a block that a successful Reconnect has just disproved", () => {
    // The regression: `recordSilentRemint` was the only recorder, and it is
    // reached from the three SILENT paths only. So a reader who pressed
    // Reconnect and completed it stayed blocked for the rest of the page's
    // life, with both silent effects bailing out and the calendar going
    // quietly stale immediately after the gesture that fixed it.
    const blocked = fold(["login_required", "login_required"]);
    expect(blocked.blocked).toBe(true);
    expect(recordInteractiveConnect(blocked, null)).toEqual(INITIAL_REMINT_HEALTH);
  });

  it("leaves the count alone when the interactive attempt failed", () => {
    // A cancelled popup or a declined consent says what the reader chose to
    // do, not whether a silent iframe could have got a token. Counting those
    // would let two cancelled Reconnects block a path that was never tried.
    const one = fold(["login_required"]);
    expect(recordInteractiveConnect(one, "popup_closed")).toEqual(one);
    expect(recordInteractiveConnect(one, "access_denied").blocked).toBe(false);
    expect(recordInteractiveConnect(INITIAL_REMINT_HEALTH, "popup_closed")).toEqual(
      INITIAL_REMINT_HEALTH,
    );
  });
});
