// The `TokenClient` (`./token-client.ts`) that replaces `google/gis.ts`
// (#577/#583): a `calendar.readonly` token minted server-side by
// `POST /api/google/calendar_token` (ADR-0028), never by the browser
// talking to Google directly.
//
// **Modeled on `skills/run-skill.ts`.** Same shape for the same reasons:
// main-thread, same-origin (ADR-0018), authenticated from the stored
// device token, never throws, and the device token never appears in any
// returned string — the only place it appears is the `authorization`
// header.
//
// **`prompt` is accepted and ignored.** That is the entire point of moving
// the mint server-side: the authority's answer does not depend on whether
// this was a silent re-mint or an interactive Connect, so
// `calendar/connection.ts` needed no code change at all — it still calls
// `requestToken("none")` and `requestToken("consent")`, and both reach the
// same request here.
//
// **No stored device token means no fetch call at all** — `no_device_token`
// comes back without touching the network, the same rule `run-skill.ts`'s
// `NO_TOKEN` follows.
//
// **A plain request timeout, not GIS's injectable seam.** `gis.ts`'s 15s/
// 120s ceilings and its `StartTimer` seam existed because a popup could
// hang forever with neither callback ever firing. A same-origin `fetch` has
// no such failure mode — an unanswered request is just slow — so
// `AbortSignal.timeout` is enough, and a rejected/aborted fetch reads as
// `authority_unreachable` like any other transport failure.
//
// **Every failure the route can produce maps to its own code** (Agent
// Brief): `authority_rejected_device_token` (401/403 — matches
// `handlers::auth::authenticate`'s empty-body ADR-0004 verdict),
// `authority_unconfigured` (503 — the three Google secrets are unset),
// `authority_upstream` (502 — unreachable/`invalid_grant`/other upstream,
// `google_calendar.rs`'s own three cases collapse to one code here because
// none of them is this client's to act on), `bad_token_response` (any other
// non-2xx, or a 200 whose body could not be read as JSON at all — a
// malformed answer, worth retrying), and `no_access_token` (valid JSON that
// nonetheless lacks a usable token — structurally wrong, not transient).

import type { TokenClient, TokenError, TokenPrompt, TokenResult } from "./token-client";

/** ADR-0028's route: same-origin (ADR-0018), `device` scope, no body. */
export const CALENDAR_TOKEN_ENDPOINT = "/api/google/calendar_token";

/** How long one request may take before it is called failed. Generous for a
 * same-origin POST that is usually answered from the Durable Object's
 * cache — this is "the network is not answering", not a UX-timed ceiling
 * the way GIS's popup timeouts were. */
export const REQUEST_TIMEOUT_MS = 15_000;

export interface AuthorityTokenClientDeps {
  fetch: typeof globalThis.fetch;
  /** The device token, or `null` when none is stored. Read on the main
   * thread from `task/token-store.ts`, the same seam `run-skill.ts` uses. */
  readToken: () => Promise<string | null>;
}

interface CalendarTokenResponseBody {
  access_token?: unknown;
  expires_at_ms?: unknown;
}

/** Total on every value `response.json()` can return — `null` is valid JSON,
 * and reading a field off it would throw out of a client documented never to.
 * A non-object body is `no_access_token` like any other unusable answer. */
function isUsableToken(
  body: unknown,
): body is { access_token: string; expires_at_ms: number } {
  if (body === null || typeof body !== "object") {
    return false;
  }
  const { access_token, expires_at_ms } = body as CalendarTokenResponseBody;
  return typeof access_token === "string" && typeof expires_at_ms === "number";
}

export function createAuthorityTokenClient(deps: AuthorityTokenClientDeps): TokenClient {
  return {
    async requestToken(_prompt: TokenPrompt): Promise<TokenResult | TokenError> {
      // `readToken` reads IndexedDB, which rejects outright when the store is
      // blocked or corrupt (private windows, a wedged upgrade). This client
      // never throws, so an unreadable store reads as "no token stored" —
      // which is what it means from here: nothing to authenticate with.
      let token: string | null;
      try {
        token = await deps.readToken();
      } catch {
        return { error: "no_device_token" };
      }
      if (token === null || token === "") {
        return { error: "no_device_token" };
      }

      let response: Response;
      try {
        response = await deps.fetch(CALENDAR_TOKEN_ENDPOINT, {
          method: "POST",
          headers: { authorization: `Bearer ${token}` },
          signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
        });
      } catch {
        return { error: "authority_unreachable" };
      }

      if (response.status === 401 || response.status === 403) {
        return { error: "authority_rejected_device_token" };
      }
      if (response.status === 503) {
        return { error: "authority_unconfigured" };
      }
      if (response.status === 502) {
        return { error: "authority_upstream" };
      }
      if (!response.ok) {
        return { error: "bad_token_response" };
      }

      let body: unknown;
      try {
        body = await response.json();
      } catch {
        return { error: "bad_token_response" };
      }

      if (!isUsableToken(body)) {
        return { error: "no_access_token" };
      }
      return { accessToken: body.access_token, expiresAtMs: body.expires_at_ms };
    },
  };
}
