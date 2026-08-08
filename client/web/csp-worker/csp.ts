// Kept in its own module, separate from worker.ts's default export: workerd
// treats every named export of a Worker's `main` module as a candidate
// handler/entrypoint, and errors on a plain string constant living there.

// No `unsafe-inline`; `connect-src` limited to self and `api.linear.app`
// (ADR-0004's mitigation for a non-expiring key in browser storage). The
// shell is keyless and mirror-free (ADR-0006), so there is nothing else to
// scope a source list to.
export const CONTENT_SECURITY_POLICY = [
  "default-src 'self'",
  "script-src 'self'",
  "style-src 'self'",
  "connect-src 'self' https://api.linear.app",
  "img-src 'self' data:",
  "manifest-src 'self'",
  "worker-src 'self'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
].join("; ");
