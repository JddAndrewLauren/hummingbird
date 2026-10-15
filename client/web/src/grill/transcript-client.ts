// The Grill history's one fetch (#358): a completed Grill's transcript, read
// from `GET /api/grills/:id` — the only place the authority ever serves one
// (ADR-0023 decision 4). The sweep carries every other column and the core
// mirror keeps them; the transcript is what that decision kept off the
// sweep, so a history entry asks for it here when it is expanded and at no
// other time.
//
// **Modeled on `calendar/authority-token-client.ts`**, for the same reasons:
// main-thread, same-origin (ADR-0018), authenticated from the stored device
// token, never throws, and no stored token means no fetch at all. It is a
// read, so it stays out of the outbound queue, and it never asks for a sync
// cycle — a grill is immutable (ADR-0023 decision 2), so nothing it returns
// can be stale relative to the mirror.

/** The route, minus the id. Same-origin (ADR-0018), `device` scope. */
export const GRILL_ENDPOINT = "/api/grills/";

/** The same "the network is not answering" ceiling
 * `authority-token-client.ts` uses — not a UX-timed limit. */
export const REQUEST_TIMEOUT_MS = 15_000;

export type TranscriptFailure =
  /** No device token is stored — nothing to authenticate with. */
  | "no_device_token"
  /** The request never got an answer (offline, aborted, timed out). */
  | "unreachable"
  /** 401/403: the authority refused this device's token. */
  | "rejected"
  /** 404: the authority has no such grill. */
  | "not_found"
  /** Any other non-2xx, or a 2xx whose body carried no transcript. */
  | "bad_response";

export type TranscriptResult =
  | { kind: "ok"; transcript: string }
  | { kind: "failed"; reason: TranscriptFailure };

export type FetchTranscript = (grillId: string) => Promise<TranscriptResult>;

export interface GrillTranscriptClientDeps {
  fetch: typeof globalThis.fetch;
  /** The device token, or `null` when none is stored — `task/token-store.ts`,
   * the seam `run-skill.ts` reads too. */
  readToken: () => Promise<string | null>;
}

export function createGrillTranscriptClient(deps: GrillTranscriptClientDeps): FetchTranscript {
  return async (grillId) => {
    let token: string | null;
    try {
      token = await deps.readToken();
    } catch {
      return { kind: "failed", reason: "no_device_token" };
    }
    if (token === null || token === "") {
      return { kind: "failed", reason: "no_device_token" };
    }

    let response: Response;
    try {
      response = await deps.fetch(`${GRILL_ENDPOINT}${encodeURIComponent(grillId)}`, {
        method: "GET",
        headers: { authorization: `Bearer ${token}` },
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
    } catch {
      return { kind: "failed", reason: "unreachable" };
    }

    if (response.status === 401 || response.status === 403) {
      return { kind: "failed", reason: "rejected" };
    }
    if (response.status === 404) {
      return { kind: "failed", reason: "not_found" };
    }
    if (!response.ok) {
      return { kind: "failed", reason: "bad_response" };
    }

    let body: unknown;
    try {
      body = await response.json();
    } catch {
      return { kind: "failed", reason: "bad_response" };
    }
    if (body === null || typeof body !== "object") {
      return { kind: "failed", reason: "bad_response" };
    }
    const { transcript } = body as { transcript?: unknown };
    if (typeof transcript !== "string") {
      return { kind: "failed", reason: "bad_response" };
    }
    return { kind: "ok", transcript };
  };
}
