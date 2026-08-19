import { describe, expect, it } from "vitest";
import { connectErrorCopy } from "./connect-error";

// `google/gis.ts` minted this code (#583 deleted that module); the case
// itself stays in `connect-error.ts` until #584 removes the whole
// interactive browser OAuth surface, so the literal moves here rather than
// being lost with its old home.
const TOKEN_TIMEOUT_ERROR = "token_request_timed_out";

// Every error the connection can produce. Kept here as a list rather than
// derived, because most of the union is GIS's and not ours — but the fallback
// is in the list too, which is the point: an unknown code must still yield real
// words, since the whole failure mode being fixed is a button that says
// nothing.
//
// The last two are NOT Google's. `google/redirect-flow.ts` mints them: this
// app's CSRF check failing, and this app refusing a token with no usable
// lifetime. They were reaching the default arm and rendering as `Google
// reported "state_mismatch".`, which sends the reader to debug the wrong
// system entirely.
const EVERY_ERROR = [
  TOKEN_TIMEOUT_ERROR,
  "popup_failed_to_open",
  "popup_closed",
  "access_denied",
  "gis_script_load_failed",
  "gis_unavailable",
  "gis_request_failed",
  "no_access_token",
  "some_code_google_invented_last_tuesday",
  "state_mismatch",
  "no_expiry",
];

/** The codes this app mints itself, and which therefore must not be reported
 * as things Google said. */
const OUR_OWN_ERRORS = ["state_mismatch", "no_expiry"];

describe("connectErrorCopy", () => {
  it.each(EVERY_ERROR)("says something, and something to do, for %s", (error) => {
    for (const standalone of [true, false]) {
      const { message, hint } = connectErrorCopy(error, standalone);
      expect(message.length).toBeGreaterThan(0);
      // The hint is the half that makes the message actionable. A hint that
      // is merely present but says nothing to DO would pass a length check,
      // so this looks for a verb the reader can follow.
      expect(hint.length).toBeGreaterThan(0);
      expect(hint).toMatch(/try again|allow|reload|check|close|press connect|remove/i);
    }
  });

  it("echoes an unrecognised code rather than swallowing it", () => {
    expect(connectErrorCopy("wat", false).message).toContain("wat");
  });

  it.each(OUR_OWN_ERRORS)("renders %s as words rather than a quoted Google error", (error) => {
    // `state_mismatch` is the CSRF check in `google/redirect-flow.ts` failing
    // and `no_expiry` is that module refusing a token it cannot schedule
    // against. Both used to fall through to the default arm, whose sentence
    // named Google as having *reported* the code — for `state_mismatch` that
    // sends the reader off to look at their Google account for a decision this
    // app made about a fragment.
    const { message, hint } = connectErrorCopy(error, false);
    expect(message).not.toContain("Google reported");
    // And the raw code is not what a reader is left holding: these two have
    // real copy, so the diagnosis is a sentence rather than an identifier.
    expect(message).not.toContain(error);
    expect(hint.length).toBeGreaterThan(0);
  });

  it("tells the reader what to do about a state mismatch, in this app", () => {
    // The actionable half. A mismatched state means "believe nothing that
    // arrived" — including any suggestion that came with the link that
    // produced it — so the next action has to point back at this screen.
    const { hint } = connectErrorCopy("state_mismatch", false);
    expect(hint).toMatch(/press connect/i);
  });

  // The one place `standalone` changes the answer. Sending someone to Safari
  // is wrong in an installed iOS app: its storage container is separate, so a
  // connection made in a tab is not one the app can see.
  it("does not send an installed app to a browser tab", () => {
    const installed = connectErrorCopy(TOKEN_TIMEOUT_ERROR, true);
    const tab = connectErrorCopy(TOKEN_TIMEOUT_ERROR, false);
    expect(installed.hint).not.toBe(tab.hint);
    expect(installed.hint.toLowerCase()).not.toContain("safari");
    expect(installed.hint).toMatch(/from here/);
  });
});
