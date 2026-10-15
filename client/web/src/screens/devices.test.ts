import { describe, expect, it } from "vitest";
import { devicesNote, otherDeviceConfirmation, revokeFailureMessage } from "./devices";

describe("devicesNote (#840)", () => {
  it("says why there are no rows, or nothing when there are", () => {
    expect(devicesNote(null)).toBe("Reading your devices.");
    expect(devicesNote({ kind: "no_credential", devices: [], error: null })).toBe(
      "This device is signed out. Sign in to see the devices signed in to hummingbird.",
    );
    expect(devicesNote({ kind: "credential_rejected", devices: [], error: null })).toMatch(/no longer works/);
    expect(devicesNote({ kind: "failed", devices: [], error: "offline" })).toBe(
      "Could not read the devices: offline",
    );
    expect(devicesNote({ kind: "listed", devices: [], error: null })).toBeNull();
  });
});

describe("otherDeviceConfirmation", () => {
  it("names the device and what signing it out means", () => {
    expect(otherDeviceConfirmation("Phone")).toBe("Sign out Phone? It stops syncing until it signs in again.");
  });
});

describe("revokeFailureMessage", () => {
  it("words every refusal", () => {
    expect(revokeFailureMessage("not_revocable", null)).toMatch(/cannot be signed out from here/);
    expect(revokeFailureMessage("not_found", null)).toMatch(/no longer signed in/);
    expect(revokeFailureMessage("no_credential", null)).toMatch(/signed out/);
    expect(revokeFailureMessage("credential_rejected", null)).toMatch(/no longer works/);
    expect(revokeFailureMessage("failed", "offline")).toBe("Could not sign it out: offline");
    expect(revokeFailureMessage("busy", null)).toMatch(/try again/i);
  });
});
