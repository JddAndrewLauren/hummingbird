import { describe, expect, it } from "vitest";
import { connectErrorCopy } from "./connect-error";
import { TOKEN_TIMEOUT_ERROR } from "../google/gis";

// Every error the connection can produce. Kept here as a list rather than
// derived, because the union is GIS's and not ours — but the fallback is in
// the list too, which is the point: an unknown code must still yield real
// words, since the whole failure mode being fixed is a button that says
// nothing.
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
];

describe("connectErrorCopy", () => {
  it.each(EVERY_ERROR)("says something, and something to do, for %s", (error) => {
    for (const standalone of [true, false]) {
      const { message, hint } = connectErrorCopy(error, standalone);
      expect(message.length).toBeGreaterThan(0);
      // The hint is the half that makes the message actionable. A hint that
      // is merely present but says nothing to DO would pass a length check,
      // so this looks for a verb the reader can follow.
      expect(hint.length).toBeGreaterThan(0);
      expect(hint).toMatch(/try again|allow|reload|check|close/i);
    }
  });

  it("echoes an unrecognised code rather than swallowing it", () => {
    expect(connectErrorCopy("wat", false).message).toContain("wat");
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
