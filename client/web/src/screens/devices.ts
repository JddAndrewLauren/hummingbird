import type { DevicesDTO, RevokeDeviceKind } from "../store/protocol";

// The Signed-in devices section's own words (#840) — the ones the core's
// devices decision does not already supply. Its rows' order and wording,
// and the sentences before this device or another signs out, are the
// core's (`decisions/seam.ts`'s `deviceRows`/`signOutConfirmation`/
// `otherDeviceSignOutConfirmation`, ADR-0040 decision 7, #887); what is
// left here is the section's states and its write
// failures, which, like every other write error on this screen, are worded
// per client.

/** The sentence standing in for the rows, or `null` when there are rows
 * to draw. */
export function devicesNote(devices: DevicesDTO | null): string | null {
  if (devices === null) {
    return "Reading your devices.";
  }
  switch (devices.kind) {
    case "listed":
      return null;
    case "no_credential":
      return "This device is signed out. Sign in to see the devices signed in to hummingbird.";
    case "credential_rejected":
      return "This device's token no longer works, so the devices cannot be read. Sign in again.";
    case "failed":
      return `Could not read the devices: ${devices.error ?? "unknown error"}`;
  }
}

/** What a sign-out that did not happen says. */
export function revokeFailureMessage(
  kind: Exclude<RevokeDeviceKind, "revoked" | "signed_out">,
  detail: string | null,
): string {
  switch (kind) {
    case "not_revocable":
      return "This device cannot be signed out from here.";
    case "not_found":
      return "That device is no longer signed in.";
    case "no_credential":
      return "This device is signed out, so it cannot sign others out.";
    case "credential_rejected":
      return "This device's token no longer works. Sign in again first.";
    case "failed":
      return `Could not sign it out: ${detail ?? "unknown error"}`;
    case "busy":
      return "The local core was busy. Try again.";
  }
}
