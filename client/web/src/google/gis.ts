// The Google Identity Services (GIS) token client (issue #73) — kept thin
// and browser-only, mirroring core.worker.ts's discipline of isolating
// untestable environment glue from the logic that consumes it
// (calendar/connection.ts, which is unit-tested against a fake
// `TokenClient`).
//
// Requests exactly `calendar.readonly` — no write scope, ever (Agent
// Brief's first acceptance criterion). This is an independent per-device
// credential: it reuses the sweeper's Workspace Internal OAuth client id
// only as the registered client id, never its refresh token or Tasks/Gmail
// scopes (those live entirely in `sweep.py`'s world, untouched by this
// module).

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
  scriptLoadPromise ??= new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = GIS_SCRIPT_SRC;
    script.async = true;
    script.onload = () => resolve();
    script.onerror = () => reject(new Error("failed to load Google Identity Services"));
    document.head.appendChild(script);
  });
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
      await loadGisScript();
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
