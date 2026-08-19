import { describe, expect, it } from "vitest";
import {
  blockingRemintErrors,
  classifyRemintError,
  INITIAL_REMINT_HEALTH,
  nonBlockingRemintErrors,
  recordInteractiveConnect,
  recordSilentRemint,
  SILENT_REMINT_FAILURE_LIMIT,
} from "./remint-health";

const NON_BLOCKING_ERRORS = ["authority_unreachable", "bad_token_response"];

/** Every code `calendar/authority-token-client.ts`'s `TokenClient` can
 * actually come back with (the Agent Brief's list). This is the list the
 * classification has to be TOTAL over: a code here that nobody has
 * classified is the accident the two sets exist to prevent, and it is not
 * caught by asserting the sets are disjoint. */
const AUTHORITY_TOKEN_CLIENT_ERRORS = [
  "no_device_token",
  "authority_unreachable",
  "authority_rejected_device_token",
  "authority_unconfigured",
  "authority_upstream",
  "bad_token_response",
  "no_access_token",
];

function fold(errors: (string | null)[]) {
  return errors.reduce(recordSilentRemint, INITIAL_REMINT_HEALTH);
}

describe("recordSilentRemint", () => {
  it("does not block on one failure", () => {
    // A single failure is routinely transient — a request that raced a
    // Durable Object cold start, a blip talking to the authority. Blocking
    // on it would turn one hiccup into a manual reconnect.
    expect(fold(["authority_rejected_device_token"])).toEqual({
      consecutiveFailures: 1,
      blocked: false,
    });
  });

  it("blocks at the limit", () => {
    const health = fold(Array<string>(SILENT_REMINT_FAILURE_LIMIT).fill("authority_unconfigured"));
    expect(health.blocked).toBe(true);
  });

  it("any success resets completely", () => {
    const recovered = fold(["authority_rejected_device_token", "authority_unconfigured", null]);
    expect(recovered).toEqual(INITIAL_REMINT_HEALTH);
    // And the count really is back to zero, not merely unblocked: one further
    // failure must not re-block immediately.
    expect(recordSilentRemint(recovered, "authority_rejected_device_token").blocked).toBe(false);
  });

  it.each(blockingRemintErrors())("counts %s toward blocking", (error) => {
    expect(fold([error, error]).blocked).toBe(true);
  });

  // The one that matters most. Blocking on these would push a merely-offline
  // reader into an interactive Connect they cannot complete anyway.
  it.each(NON_BLOCKING_ERRORS)("never blocks on %s, however many times it happens", (error) => {
    expect(fold(Array<string>(20).fill(error))).toEqual(INITIAL_REMINT_HEALTH);
  });

  it("a non-blocking failure neither counts nor forgives a real run", () => {
    // It is not evidence the authority recovered, so the run it interrupts
    // stands — one blocking failure either side still blocks.
    expect(
      fold(["authority_rejected_device_token", "authority_unreachable", "authority_unconfigured"])
        .blocked,
    ).toBe(true);
  });

  it("a revoked refresh token would 502 every hour forever, and counts", () => {
    // `authority_upstream` is what a dead/revoked GOOGLE_CALENDAR_REFRESH_TOKEN
    // (ADR-0028) looks like from here — the module header's whole reason to
    // still exist. Retrying it hourly forever is exactly the nag this limit
    // stops.
    expect(fold(["authority_upstream", "authority_upstream"]).blocked).toBe(true);
  });

  it("retries nothing whose documented remedy is a human, no_access_token included", () => {
    // `connect-error.ts` tells this reader what to do about a token-shaped
    // answer with no usable token. It must not be retried hourly forever.
    expect(classifyRemintError("no_access_token")).toBe("blocking");
    expect(fold(["no_access_token", "no_access_token"]).blocked).toBe(true);
  });

  it("classifies every error the authority token client can produce, and none twice", () => {
    for (const error of AUTHORITY_TOKEN_CLIENT_ERRORS) {
      // Totality: `unclassified` here means a code reached the reducer and
      // got its behaviour by falling off the end of both sets rather than by
      // a decision. That is what this file used to allow.
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
    // Every code the token client can produce is one of the two sets — no
    // code the silent path can actually see is `unclassified`.
    const classified = new Set([...blockingRemintErrors(), ...nonBlockingRemintErrors()]);
    for (const error of AUTHORITY_TOKEN_CLIENT_ERRORS) {
      expect(classified.has(error)).toBe(true);
    }
  });

  it("makes the reducer agree with the classification, code by code", () => {
    // The classification is only worth anything if it is what the counter
    // actually consults. Driven off the sets themselves, so a code added to
    // one of them without the reducer honouring it fails here.
    for (const error of AUTHORITY_TOKEN_CLIENT_ERRORS) {
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
    const blocked = fold(["authority_unconfigured", "authority_unconfigured"]);
    expect(blocked.blocked).toBe(true);
    expect(recordInteractiveConnect(blocked, null)).toEqual(INITIAL_REMINT_HEALTH);
  });

  it("leaves the count alone when the interactive attempt failed", () => {
    const one = fold(["authority_unconfigured"]);
    expect(recordInteractiveConnect(one, "authority_rejected_device_token")).toEqual(one);
    expect(recordInteractiveConnect(one, "no_access_token").blocked).toBe(false);
    expect(recordInteractiveConnect(INITIAL_REMINT_HEALTH, "authority_unreachable")).toEqual(
      INITIAL_REMINT_HEALTH,
    );
  });
});
