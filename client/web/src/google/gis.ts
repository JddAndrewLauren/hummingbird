// The Google Identity Services (GIS) token client (issue #73) — kept thin
// and browser-only, mirroring core.worker.ts's discipline of isolating
// untestable environment glue from the logic that consumes it
// (calendar/connection.ts, which is unit-tested against a fake
// `TokenClient`).
//
// Requests exactly `calendar.readonly` — no write scope, ever (Agent
// Brief's first acceptance criterion). This is an independent per-device
// credential, minted by its own Web-application OAuth client (see
// `.env.example`: the sweeper's client is a desktop-app one and cannot be
// reused for a browser flow, contrary to issue #73's brief). The sweeper's
// refresh token and Tasks/Gmail scopes are untouched either way — those
// live entirely in `sweep.py`'s world, not this module's.
//
// **Every request has a ceiling.** `initTokenClient` is popup-based with no
// `ux_mode` option, and in an installed iOS PWA the popup escapes to Safari
// and loses its opener, so neither `callback` nor `error_callback` ever
// fires — the returned promise simply never settled and the Connect button
// did nothing, visibly and forever. The `StartTimer` seam below is what ends
// that wait; `calendar/connect-error.ts` turns the resulting code into words.

export const CALENDAR_READONLY_SCOPE =
  "https://www.googleapis.com/auth/calendar.readonly";

/** How long a `prompt: "none"` request may hang before it is called failed.
 * It is a hidden iframe round-trip with no user in it, so 15s is generous;
 * under iOS ITP the iframe loads and never posts back at all, and this is the
 * only thing that ends that wait. */
export const SILENT_TOKEN_TIMEOUT_MS = 15_000;

/** The interactive ceiling. Long, because a real person is reading a consent
 * screen, possibly signing in and possibly choosing an account — two minutes
 * is "this will never come back", not "you are slow". */
export const INTERACTIVE_TOKEN_TIMEOUT_MS = 120_000;

/** The `TokenError.error` a timed-out request reports. Its own constant
 * because two modules match on it: `calendar/connect-error.ts` (what to tell
 * the user) and `calendar/remint-health.ts` (a silent timeout is the ITP
 * signature and counts toward blocking). */
export const TOKEN_TIMEOUT_ERROR = "token_request_timed_out";

/** Schedules `run` and returns a canceller. Injected so the timeout is
 * testable without a real clock — the `now` seam below is the same idea. */
export type StartTimer = (run: () => void, delayMs: number) => () => void;

const DEFAULT_START_TIMER: StartTimer = (run, delayMs) => {
  const id = setTimeout(run, delayMs);
  return () => clearTimeout(id);
};

export type TokenPrompt = "" | "none" | "consent";

export interface TokenResult {
  accessToken: string;
  /** Milliseconds since the Unix epoch. GIS reports `expires_in` seconds
   * from "now"; the caller supplies "now" so this stays testable/pure at
   * the call site rather than sampling `Date.now()` in here. */
  expiresAtMs: number;
}

export interface TokenError {
  error: string;
}

export interface TokenClient {
  requestToken(prompt: TokenPrompt): Promise<TokenResult | TokenError>;
}

function isTokenError(value: TokenResult | TokenError): value is TokenError {
  return "error" in value;
}

export function isTokenResult(value: TokenResult | TokenError): value is TokenResult {
  return !isTokenError(value);
}

// -- GIS script loading + the real, browser-only TokenClient ---------------

const GIS_SCRIPT_SRC = "https://accounts.google.com/gsi/client";

interface GisTokenResponse {
  access_token?: string;
  expires_in?: number;
  error?: string;
}

interface GisTokenClient {
  requestAccessToken(overrides?: { prompt?: TokenPrompt }): void;
}

interface GisAccounts {
  oauth2: {
    initTokenClient(config: {
      client_id: string;
      scope: string;
      callback: (response: GisTokenResponse) => void;
      error_callback?: (error: { type: string; message?: string }) => void;
    }): GisTokenClient;
  };
}

declare global {
  interface Window {
    google?: { accounts: GisAccounts };
  }
}

let scriptLoadPromise: Promise<void> | null = null;

function loadGisScript(): Promise<void> {
  if (window.google?.accounts) {
    return Promise.resolve();
  }
  if (scriptLoadPromise === null) {
    const script = document.createElement("script");
    script.src = GIS_SCRIPT_SRC;
    script.async = true;
    scriptLoadPromise = new Promise<void>((resolve, reject) => {
      // Handlers before `appendChild`, so a synchronous failure cannot fire
      // into a script with nothing listening.
      script.onload = () => resolve();
      script.onerror = () => reject(new Error("failed to load Google Identity Services"));
      document.head.appendChild(script);
    }).catch((error: unknown) => {
      // A rejected load must NOT stay memoised. The common failure here is
      // transient (offline, blocked, CDN hiccup), and a cached rejection
      // would make every later Connect/Reconnect click replay that same
      // failure until the page is reloaded, long after connectivity came
      // back — the button would look alive and be dead. Clearing the slot
      // (and removing the spent tag, so a retry appends a fresh one) makes
      // the next call a real retry. The success path stays memoised, so
      // exactly one script tag ever loads.
      scriptLoadPromise = null;
      script.remove();
      throw error;
    });
  }
  return scriptLoadPromise;
}

/** Builds the real, browser-only [`TokenClient`], requesting exactly
 * `calendar.readonly`. `now` and `startTimer` are injected (defaulting to
 * `Date.now` and `setTimeout`) so the `expiresAtMs` computation and the
 * timeout stay overridable in the one place each matters for a real run. */
export function createGisTokenClient(
  clientId: string,
  now: () => number = Date.now,
  startTimer: StartTimer = DEFAULT_START_TIMER,
): TokenClient {
  return {
    async requestToken(prompt: TokenPrompt): Promise<TokenResult | TokenError> {
      try {
        await loadGisScript();
      } catch {
        // A script-load failure (offline, blocked, CDN down) must resolve
        // to the documented `TokenError` union member, same as any other
        // GIS failure -- callers (`calendar/connection.ts`) never `catch`
        // `requestToken`, so letting this reject would surface as an
        // unhandled rejection and leave the UI stuck showing an inert
        // Connect/Reconnect button instead of routing to `needsReconnect`.
        return { error: "gis_script_load_failed" };
      }
      const accounts = window.google?.accounts;
      if (!accounts) {
        return { error: "gis_unavailable" };
      }
      return new Promise((resolve) => {
        const startMs = now();
        // Three things can settle this promise — `callback`, `error_callback`
        // and the timeout — and before the timeout existed the first two were
        // assumed to be exhaustive. They are not. `initTokenClient` is
        // popup-based with no `ux_mode` option, and in an installed iOS PWA
        // the popup escapes to Safari and loses its opener: NEITHER callback
        // ever fires, and this promise simply never settled. The Connect
        // button did nothing, visibly and permanently, which is the bug this
        // seam exists for.
        //
        // One `settle` for all three, cancelling the timer on the first, so a
        // late callback after a timeout cannot resolve a settled promise or
        // leave a timer running past it.
        let settled = false;
        let cancelTimer: (() => void) | null = null;
        function settle(value: TokenResult | TokenError) {
          if (settled) {
            return;
          }
          settled = true;
          cancelTimer?.();
          resolve(value);
        }

        // Started BEFORE `requestAccessToken`, not after: opening the popup is
        // the step that can hang, and a timer armed afterwards would never be
        // armed at all if that call blocked or threw.
        //
        // `setTimeout` is suspended while a standalone window is backgrounded,
        // so on the escape path above this fires when the user comes back to
        // the app — which is exactly when they are looking at it, and the only
        // moment the message is any use. That is intended, not a limitation.
        cancelTimer = startTimer(
          () => settle({ error: TOKEN_TIMEOUT_ERROR }),
          prompt === "none" ? SILENT_TOKEN_TIMEOUT_MS : INTERACTIVE_TOKEN_TIMEOUT_MS,
        );

        // GIS's `initTokenClient` takes one fixed callback at construction
        // time; building a fresh client per request (rather than reusing
        // one) is what lets each call resolve its own promise instead of
        // racing a shared callback across concurrent requests.
        try {
          const client = accounts.oauth2.initTokenClient({
            client_id: clientId,
            scope: CALENDAR_READONLY_SCOPE,
            callback: (response) => {
              if (!response.access_token) {
                settle({ error: response.error ?? "no_access_token" });
                return;
              }
              const expiresInMs = (response.expires_in ?? 0) * 1000;
              settle({
                accessToken: response.access_token,
                expiresAtMs: startMs + expiresInMs,
              });
            },
            error_callback: (error) => {
              settle({ error: error.type });
            },
          });
          client.requestAccessToken({ prompt });
        } catch {
          // A synchronous throw here escapes the executor as an unhandled
          // rejection and leaves the caller waiting on a promise that will
          // never settle — the same dead button by another route. Route it
          // into the documented union instead.
          settle({ error: "gis_request_failed" });
        }
      });
    },
  };
}
