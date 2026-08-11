import { timingSafeEqual } from "node:crypto";

const BEARER_PREFIX = "Bearer ";

/**
 * Checks an `Authorization` header against the runner's single static
 * bearer token, constant-time (#41 decision 5: "one user, constant-time
 * compare"). An empty configured token never matches anything, even an
 * empty-string header — there is no such thing as an unset token that
 * authenticates.
 *
 * @param {string | undefined} header
 * @param {string} expectedToken
 * @returns {boolean}
 */
export function checkBearerToken(header, expectedToken) {
  if (!expectedToken) return false;
  if (typeof header !== "string" || !header.startsWith(BEARER_PREFIX)) {
    return false;
  }
  const provided = header.slice(BEARER_PREFIX.length);

  const a = Buffer.from(provided);
  const b = Buffer.from(expectedToken);
  // timingSafeEqual throws on length mismatch, so compare against a
  // same-length buffer first -- the length check itself is not
  // secret-dependent (Fly's own TLS termination time already leaks
  // request presence), only the token bytes must not be.
  if (a.length !== b.length) return false;
  return timingSafeEqual(a, b);
}
