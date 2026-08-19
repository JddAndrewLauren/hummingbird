// The calendar token-client contract (issue #73, moved out of
// `google/gis.ts` by #583). `calendar/connection.ts` is written against
// this interface alone and is unit-tested with a fake — nothing here says
// where a token comes from, on purpose: `calendar/authority-token-client.ts`
// is the current (and, per #577, only) implementation, minting from the
// authority rather than from Google directly in the browser.
//
// `prompt` survives on the interface even though the current implementation
// ignores it (#577/#583's whole point: the authority-backed client needs no
// interactive/silent distinction, so `connection.ts` needed no change at
// all). It stays because `connection.ts` still calls `requestToken("none")`
// for a silent re-mint and `requestToken("consent")` for an interactive one,
// and a future implementation is free to care again.

export type TokenPrompt = "" | "none" | "consent";

export interface TokenResult {
  accessToken: string;
  /** Milliseconds since the Unix epoch, absolute — not a duration. */
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
