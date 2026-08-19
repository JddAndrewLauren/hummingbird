import { describe, expect, it } from "vitest";
import { connectErrorCopy } from "./connect-error";

// Every error `calendar/authority-token-client.ts`'s `TokenClient` can
// produce — its own header lists and explains these seven — plus a code
// nobody has seen, which must still render real words: the whole failure
// mode being fixed is a button that says nothing.
const EVERY_ERROR = [
  "no_device_token",
  "authority_rejected_device_token",
  "authority_unconfigured",
  "authority_upstream",
  "authority_unreachable",
  "bad_token_response",
  "no_access_token",
];

describe("connectErrorCopy", () => {
  it.each(EVERY_ERROR)("says something, and something to do, for %s", (error) => {
    const { message, hint } = connectErrorCopy(error);
    expect(message.length).toBeGreaterThan(0);
    // The hint is the half that makes the message actionable. A hint that
    // is merely present but says nothing to DO would pass a length check,
    // so this looks for a verb the reader can follow.
    expect(hint.length).toBeGreaterThan(0);
    expect(hint).toMatch(/try again|check|remove/i);
  });

  /** A rejection that arrives through the calendar lane leaves the device-token
   * card in `resting` (`task/token-ui.ts` derives it from the *task* lane's
   * `needsReconnect`), and `SettingsScreen` renders only "Forget token" in
   * that state. So the hint must not send the reader to an entry form that is
   * not on screen — Forget is the gesture that is, and it reveals the form. */
  it("sends a rejected device token through Forget, not straight to a form", () => {
    const { hint } = connectErrorCopy("authority_rejected_device_token");
    expect(hint).toMatch(/forget/i);
  });

  it("echoes an unrecognised code rather than swallowing it", () => {
    const { message, hint } = connectErrorCopy("some_code_nobody_has_seen");
    expect(message).toContain("some_code_nobody_has_seen");
    expect(hint.length).toBeGreaterThan(0);
  });
});
