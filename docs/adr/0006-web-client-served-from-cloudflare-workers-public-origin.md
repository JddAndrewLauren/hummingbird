# ADR-0006: The web client is served from Cloudflare Workers static assets at a public, permanent origin

**Status:** accepted · 2026-08-08
**Context:** the hosting grilling of 2026-08-08, wayfinder map
[#35](https://github.com/JddAndrewLauren/hummingbird/issues/35) ticket
[#54](https://github.com/JddAndrewLauren/hummingbird/issues/54), graduated from
ticket [#49](https://github.com/JddAndrewLauren/hummingbird/issues/49). Builds
on [ADR-0003](0003-one-rust-sync-core-embedded-per-device.md) (Vite PWA client,
service worker load-bearing for offline reads) and
[ADR-0004](0004-client-linear-credential-is-scoped-per-device-host-supplied.md)
(per-device scoped key; strict CSP as the shell's recorded defense).

## Decision

**The desktop web client is served as Cloudflare Workers static assets at
`hb.twinion.net`.** `twinion.net` is already on Cloudflare nameservers;
Cloudflare steers new projects to Workers static assets over Pages; static
asset requests are free. The client is pure static files — ADR-0003
deliberately avoided COOP/COEP, so HTTPS, correct MIME types, and the CSP
header are the whole serving requirement. Fly keeps its existing role:
processes (the sweeper, the future skill-runner), never static assets.

**`hb.twinion.net` is the permanent origin, chosen now.** The origin is
load-bearing state: the device's reconciling mirror (IndexedDB), the PWA
install, and the web client's stored API key are all keyed to it. Changing
origin later means every device re-onboards from scratch — so the first
shipped origin is the forever origin, on a domain we control rather than a
provider hostname.

**The origin is public — no identity gate.** No Cloudflare Access, and no
pretense of obscurity (Certificate Transparency logs publish every subdomain
at cert issuance, so an "unguessable origin" does not exist). The defenses are
the ones already decided: the credential is per-device, `Write`-scoped,
ION-limited, and individually revocable (ADR-0004); the app shell holds no
secrets; the strict CSP is the shell's defense and **ships as a served
response header**, not a meta tag. Cloudflare's zone-level defaults handle
noise.

Access was rejected on evidence, not taste. It would place a session that
expires monthly at best directly in the path of the *hard* offline
requirement, with two documented PWA failure modes that match this app's exact
usage profile: browsers fetch `manifest.webmanifest` credentialless, so Access
redirects it to login and blocks PWA installation; and on session expiry the
service worker's fetches get CORS-blocked auth redirects — the app silently
breaks, or worse, caches the login page as the shell. Its security delta here
is only "strangers cannot read a keyless static shell." **Flip condition:**
if a future decision embeds anything secret-bearing or sensitive in the served
assets, revisit Access — and de-risk the service-worker/manifest interaction
with a prototype ticket *before* enabling it.

**CI is two test-gated workflows split by `paths:`.**

- `deploy.yml` gains a `paths:` filter scoping it to the sweeper's files
  (`sweep.py`, `tests/**`, `Dockerfile`, `fly.toml`, the crontab, and itself).
  Nothing else about it changes; its no-`schedule:` invariant stands.
- A new `deploy-client.yml`, push-to-main filtered to `client/**` and itself:
  pnpm install → typecheck/tests → Vite build (including the wasm core build
  once it exists) → `wrangler deploy`, behind the tests exactly as the sweeper
  deploys, with its own concurrency group. Secrets: a `CLOUDFLARE_API_TOKEN`
  scoped to Workers deploys plus the account id.
- `wrangler.toml` lives in `client/web/` as checked-in source of truth: the
  `hb.twinion.net` custom-domain route, the CSP header on served assets, and
  SPA fallback.

Cloudflare's own git integration (Workers Builds) was rejected: it runs the
build on an opaque builder that is awkward for the Rust→wasm toolchain, puts
no test gate in front of deploy, and splits the repo's CI across two systems.

## Why

- **The offline requirement outranks defense-in-depth for a keyless shell.**
  Offline reads are a hard requirement of map #35 and the service worker is
  load-bearing (ADR-0003). An identity gate couples that requirement to a
  monthly-expiring session for near-zero security gain — everything valuable
  is already defended by ADR-0004's credential posture and CSP.
- **Asymmetric reversibility.** Adding Access later is a Cloudflare config
  change; the debugging time it would cost meanwhile is not recoverable.
- **Provider boundaries stay legible.** Cloudflare holds static assets and
  DNS; Fly holds processes; GitHub Actions holds the one test-gated deploy
  pipeline, now correctly split so client pushes and sweeper pushes deploy
  independently.
