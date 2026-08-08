// Kept in its own module, separate from worker.ts's default export: workerd
// treats every named export of a Worker's `main` module as a candidate
// handler/entrypoint, and errors on a plain string constant living there.

// No `unsafe-inline`; `connect-src` limited to self, `api.linear.app`
// (ADR-0004's mitigation for a non-expiring key in browser storage),
// `www.googleapis.com` (issue #73: the calendarList fetch in
// `calendar/calendarList.ts` and the wasm transport's Calendar Events
// fetch in `core::calendar::google::reqwest_transport` both target that
// host only -- no broader `googleapis.com` wildcard), and
// `accounts.google.com`. That last one is easy to miss: GIS is not only a
// script and an iframe, it also issues its own XHRs to `accounts.google.com`
// while minting a token, so a `connect-src` that omits it blocks consent and
// silent re-mint even though `script-src`/`frame-src` allow the origin --
// Google documents all four directives together
// (developers.google.com/identity/gsi/web/guides/get-google-api-clientid).
// The shell is keyless and mirror-free otherwise (ADR-0006), so there is
// nothing else to scope a source list to.
//
// `script-src` carries `'wasm-unsafe-eval'`: Chrome (and other browsers
// following the WebAssembly/CSP integration spec) require it before
// WebAssembly.compile/instantiate is allowed to run at all, even from a
// same-origin `'self'` script. It is narrower than `'unsafe-eval'` -- it
// permits wasm compilation only, not arbitrary string-to-code eval -- so it
// does not weaken the no-`unsafe-inline` intent above. It also carries
// `https://accounts.google.com`: issue #73's GIS token client
// (`google/gis.ts`) loads `https://accounts.google.com/gsi/client` as a
// same-document `<script>` tag, which needs an explicit script-src grant.
//
// `frame-src` (new) admits exactly `https://accounts.google.com`: GIS's
// silent re-mint (`prompt: "none"`) round-trips through a hidden iframe
// served from that origin -- `default-src` alone does not cover frames
// once `frame-src` is unset elsewhere, and `frame-ancestors` governs the
// opposite direction (who may frame *us*), not this.
export const CONTENT_SECURITY_POLICY = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval' https://accounts.google.com",
  "style-src 'self'",
  "connect-src 'self' https://api.linear.app https://www.googleapis.com https://accounts.google.com",
  "img-src 'self' data:",
  "manifest-src 'self'",
  "worker-src 'self'",
  "frame-src https://accounts.google.com",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
].join("; ");
