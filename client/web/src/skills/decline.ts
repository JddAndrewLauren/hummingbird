// The words for a run that never reached the seam (#273).
//
// **Only for when the transport speaks.** A 200 whose terminal line says
// `ok:false` does not come here: that is the seam declining, and its prose
// is rendered verbatim (`run-state.ts` takes `line.error` as-is). #307 made
// the seam's decline prose-only, with no machine-readable reason code,
// precisely so nothing string-matches it — prefixing or rewording it here
// would be the same mistake in a different place.
//
// So this module covers exactly three cases the seam never sees: no
// credential to send, a `fetch` that rejected, and a response whose body
// this client could not read as a terminal line.

/** No device token stored. The one decline that names an action, because
 * the user has one to take and the app can point at it. */
export const NO_TOKEN =
  "No device token on this device. Enter one in Settings, then try again.";

/**
 * The stream ended with no terminal line — a connection dropped mid-run, or
 * a body that was not what it claimed. The work may or may not have landed:
 * the runner writes to the authority, so a checklist can still appear at the
 * next sync. Said plainly rather than guessed at either way.
 */
export const NO_TERMINAL_LINE = "The run ended without an answer.";

/** A `fetch` that rejected — offline, DNS, a connection reset. */
export function declineForTransport(detail: string): string {
  const trimmed = detail.trim();
  return trimmed.length > 0
    ? `Could not reach the server: ${trimmed}`
    : "Could not reach the server.";
}

/**
 * A non-200 whose body carried no readable terminal line. The proxy answers
 * its own failures *with* one (ADR-0018's status table), so in practice this
 * is the 401/403 — empty bodies by design — and anything the platform
 * generated in front of the worker.
 *
 * A 401 is the only status worth naming as itself: it means this device's
 * token, which the user can fix. Everything else is reported as the number
 * it was, because inventing a cause for a 500 would be a guess.
 */
export function declineForResponse(status: number): string {
  if (status === 401) return "Your device token was rejected. Re-enter it in Settings.";
  if (status === 403) return "This device's token is not allowed to run skills.";
  return `The server answered ${status}.`;
}
