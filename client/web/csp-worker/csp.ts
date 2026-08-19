// Kept in its own module, separate from worker.ts's default export: workerd
// treats every named export of a Worker's `main` module as a candidate
// handler/entrypoint, and errors on a plain string constant living there.

// No `unsafe-inline`; `connect-src` limited to self (the task API is
// same-origin at `hb.twinion.net/api/*` per ADR-0008 -- the `api.linear.app`
// allowance was retired with it, since the Linear client adapter is never
// built) and `www.googleapis.com` (issue #73: both Google reads -- Calendar
// Events and the picker's calendarList -- go out from the wasm core's one
// transport, `core::calendar::google::reqwest_transport`, and target that
// host only -- no broader `googleapis.com` wildcard). The shell is keyless
// and mirror-free otherwise (ADR-0006), so there is nothing else to scope a
// source list to.
//
// `script-src` carries `'wasm-unsafe-eval'`: Chrome (and other browsers
// following the WebAssembly/CSP integration spec) require it before
// WebAssembly.compile/instantiate is allowed to run at all, even from a
// same-origin `'self'` script. It is narrower than `'unsafe-eval'` -- it
// permits wasm compilation only, not arbitrary string-to-code eval -- so it
// does not weaken the no-`unsafe-inline` intent above.
//
// #577/#586: `accounts.google.com` is gone from every directive that used to
// carry it (`script-src`, `connect-src`, `frame-src`). Nothing in the
// browser talks to Google's OAuth endpoints any more -- the authority mints
// the calendar token server-side and serves it over `POST
// /api/google/calendar_token` (ADR-0028) -- so GIS's script tag, its own
// XHRs while minting, and its hidden re-mint iframe are all gone with it.
// `www.googleapis.com` in `connect-src` STAYS: the wasm core still polls
// Google Calendar directly under ADR-0005, unchanged by ADR-0028, and that
// allowance is what its calls use.
//
// `frame-src` is explicit `'none'`, not simply removed: dropping the
// directive falls back to `default-src 'self'` for frames, which would let
// the app frame itself. There is no framing need left to grant.
export const CONTENT_SECURITY_POLICY = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "style-src 'self'",
  "connect-src 'self' https://www.googleapis.com",
  "img-src 'self' data:",
  "manifest-src 'self'",
  "worker-src 'self'",
  "frame-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
].join("; ");
