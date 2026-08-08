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

export const CALENDAR_READONLY_SCOPE =
  "https://www.googleapis.com/auth/calendar.readonly";

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
 * `calendar.readonly`. `now` is injected (defaults to `Date.now`) so the
 * `expiresAtMs` computation stays overridable in the one place it matters
 * for a real run. */
export function createGisTokenClient(
  clientId: string,
  now: () => number = Date.now,
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
        // GIS's `initTokenClient` takes one fixed callback at construction
        // time; building a fresh client per request (rather than reusing
        // one) is what lets each call resolve its own promise instead of
        // racing a shared callback across concurrent requests.
        const client = accounts.oauth2.initTokenClient({
          client_id: clientId,
          scope: CALENDAR_READONLY_SCOPE,
          callback: (response) => {
            if (!response.access_token) {
              resolve({ error: response.error ?? "no_access_token" });
              return;
            }
            const expiresInMs = (response.expires_in ?? 0) * 1000;
            resolve({
              accessToken: response.access_token,
              expiresAtMs: startMs + expiresInMs,
            });
          },
          error_callback: (error) => {
            resolve({ error: error.type });
          },
        });
        client.requestAccessToken({ prompt });
      });
    },
  };
}
