// The pure half of the capture extension (ADR-0037): everything decidable
// without a `chrome`/`browser` global, so `node --test` covers it and the
// two page scripts (`popup.js`, `options.js`) hold only DOM wiring.
//
// **This is a third copy of two rules the core owns**, re-stated here on
// purpose. `client/core/src/decisions/share.rs` decides what a shared
// payload seeds a capture with; its inputs are a `text/plain` share, ours
// are a tab's `{title, url}`, and reaching it through the wasm seam would
// pull `client/ffi-web`'s build into a directory that must stay a plain,
// dependency-free load-unpacked tree. So `urlHost` is a line-for-line port
// of `share.rs`'s `url_host`, and `draftFromTab` applies its two title
// rules — **a title is never a raw URL**, and an empty title falls back to
// the host — with `lib.test.js` carrying the same cases `share.rs`'s own
// tests do. If `url_host` changes, this changes.
//
// The wire shape is `CreateItem` (`server/domain/src/api.rs`), which is
// `deny_unknown_fields`: `buildBody` emits exactly the keys the route
// knows and nothing else. The three destinations are the web capture
// box's own (`client/web/src/screens/capture-destination.ts`): Triage,
// the default, sends no `stage` at all — the route defaults it, and
// stating it would be a second copy of a fact the server owns (sweep.py's
// `hb_create_item` says the same); the two mints send `stage: "ready"`,
// and "mint for today" adds today's local date as the deadline
// (`todayDeadline`, the web's own spelling). `link_label` is never sent:
// the host stands in for a nameless Link (`link_display_label`), exactly
// as an Android share lands.
//
// `classify` is sweep.py's response model, content-type first and status
// second: the authority shares an origin with the PWA, so an unmatched
// path is answered by the static shell with a 200, and Cloudflare's
// Browser Integrity Check answers 403 in HTML — neither is what its status
// says, and only the body can tell.

/// Where every request goes. A constant, not a setting: one authority
/// (ADR-0008), and the manifest's `host_permissions` names the same host —
/// changing one without the other yields a CORS refusal, not a redirect.
export const AUTHORITY_BASE = "https://hb.twinion.net";

/// How long a minted-but-unsaved id is reused for the same URL. A popup is
/// destroyed the moment it loses focus, so a Save whose fetch was in
/// flight may have landed; reopening on the same page within this window
/// replays the same id and the route's idempotency answers 200, not a
/// second item.
export const PENDING_TTL_MS = 10 * 60 * 1000;

/// `hb_` + 64 hex (handlers/admin_tokens.rs), 67 bytes. The one shape
/// check the options page makes, so a `\r`-tailed paste from a Windows
/// clipboard is refused here rather than presenting as a 401 later.
const TOKEN_SHAPE = /^hb_[0-9a-fA-F]{64}$/;

/// `url` minus its `http(s)://` scheme, matched case-insensitively; `null`
/// when it has no such scheme.
function stripHttpScheme(url) {
  if (typeof url !== "string") return null;
  const head = url.slice(0, 8).toLowerCase();
  if (head === "https://") return url.slice(8);
  if (head.startsWith("http://")) return url.slice(7);
  return null;
}

/// The host of an `http(s)` URL as a person would name it: the authority
/// minus any userinfo, minus the port, minus a leading `www.`, lowercased.
/// `null` for anything that is not an `http(s)://` URL with a host.
export function urlHost(url) {
  const afterScheme = stripHttpScheme(url);
  if (afterScheme === null) return null;
  const end = afterScheme.search(/[/?#]/);
  const authority = end === -1 ? afterScheme : afterScheme.slice(0, end);
  const at = authority.lastIndexOf("@");
  const hostPort = at === -1 ? authority : authority.slice(at + 1);
  let host;
  if (hostPort.startsWith("[")) {
    // An IPv6 literal is bracketed and full of colons: the host is the
    // brackets and everything inside them, and only a `:port` after the
    // closing bracket is dropped.
    const close = hostPort.indexOf("]");
    if (close === -1) return null;
    const port = hostPort.slice(close + 1);
    const portOk = port === "" || (port.startsWith(":") && /^[0-9]*$/.test(port.slice(1)));
    if (!portOk) return null;
    host = hostPort.slice(0, close + 1);
  } else {
    const colon = hostPort.lastIndexOf(":");
    host = colon !== -1 && /^[0-9]*$/.test(hostPort.slice(colon + 1)) ? hostPort.slice(0, colon) : hostPort;
  }
  if (host.startsWith("www.")) host = host.slice(4);
  // ASCII-only, as the Rust's `to_ascii_lowercase` is: a non-ASCII host
  // (unreachable through `tab.url`, which is punycode) is left alone.
  return host === "" ? null : host.replace(/[A-Z]+/g, (run) => run.toLowerCase());
}

/// Whether a tab's URL is one an item may carry as its Link: an `http(s)://`
/// URL with a host, and nothing else — `share.rs`'s `is_followable_link`.
/// `chrome://`, `about:`, `file://` and the like are refused up front, so
/// nothing unfollowable is ever written.
export function isCapturableUrl(url) {
  return urlHost(url) !== null;
}

/// Whether a trimmed title is nothing but one `http(s)` URL — the
/// `subject_is_url` rule. Wider than `share.rs`'s exact-token match on
/// purpose: a tab whose title is its own URL (a PDF, a raw text file,
/// `view-source:`) must fall back to the host, and erring toward that is
/// erring toward the rule's own reason.
function looksLikeUrl(text) {
  return /^https?:\/\/\S+$/i.test(text);
}

/// What the popup seeds its form with from the active tab, or the reason
/// it cannot. Mirrors the share mapping: the title is the tab's when it
/// has one that is not itself a URL, else the host.
export function draftFromTab(tab) {
  const url = typeof tab?.url === "string" ? tab.url : "";
  if (url === "") return { problem: "This tab cannot be read." };
  const host = urlHost(url);
  if (host === null) return { problem: "Only http(s) pages can be captured." };
  const raw = typeof tab.title === "string" ? tab.title.trim() : "";
  const title = raw === "" || looksLikeUrl(raw) ? host : raw;
  return { title, host, url };
}

/// The form's one gate, the same one every capture form has: a blank title
/// does not submit.
export function canSubmit(title) {
  return typeof title === "string" && title.trim() !== "";
}

/// The two stages a new capture may be born into — the web capture box's
/// closed vocabulary. `triage` is the default and is not sent; `ready` is
/// the glossary's Mint, for a thing already startable.
export const DESTINATIONS = ["triage", "ready"];

/// Today's local date as `YYYY-MM-DD`, the date-only deadline "mint for
/// today" stamps — `capture-meta.ts`'s `todayDeadline`, verbatim. Local,
/// not UTC: the deadline is the operator's day.
export function todayDeadline(nowMs) {
  const d = new Date(nowMs);
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/// The `CreateItem` body, and only its keys. `description` is present
/// only with content — the DTO treats absent and `null` alike, and an
/// empty string would be a stored empty description. `stage` is present
/// only for a mint; `deadline` only when one was stamped.
export function buildBody({ id, title, description, url, destination = "triage", deadline }) {
  if (!DESTINATIONS.includes(destination)) throw new Error(`not a capture destination: ${destination}`);
  const body = { id, title: title.trim(), link_url: url };
  const text = typeof description === "string" ? description.trim() : "";
  if (text !== "") body.description = text;
  if (destination === "ready") body.stage = "ready";
  if (typeof deadline === "string" && deadline !== "") body.deadline = deadline;
  return body;
}

/// The route's own charset (`server/domain/src/id.rs`): non-empty, not `.`
/// or `..`, only `[A-Za-z0-9-._~]`. `crypto.randomUUID()` always passes;
/// this is a tripwire, not a filter.
export function isValidId(id) {
  return typeof id === "string" && id !== "." && id !== ".." && /^[A-Za-z0-9\-._~]+$/.test(id);
}

/// Whether the row a 200 replay returned is the capture that was just
/// sent. A replayed id answers with the *stored* row, whatever the new
/// payload said, so "Already saved" is only true when the two agree; a
/// different title, description or stage means an earlier popup's save
/// landed and this one is a new capture that needs a fresh id.
export function rowMatches(row, body) {
  if (!row || typeof row !== "object") return false;
  const stored = (v) => (typeof v === "string" ? v : "");
  return (
    stored(row.title) === body.title &&
    stored(row.description) === (body.description ?? "") &&
    stored(row.link_url) === body.link_url &&
    stored(row.stage) === (body.stage ?? "triage")
  );
}

/// Reuse a persisted pending id for the same URL inside the TTL, else
/// `null`. A clock that went backwards is treated as expired.
export function reusablePending(pending, url, nowMs) {
  if (!pending || typeof pending.id !== "string" || pending.url !== url) return null;
  const age = nowMs - pending.mintedAt;
  if (!(age >= 0 && age < PENDING_TTL_MS)) return null;
  return pending.id;
}

/// `null` when `token` is the shape the authority mints, else the message
/// the options page shows.
export function tokenProblem(token) {
  if (typeof token !== "string" || token === "") return "Paste the token from 1Password.";
  if (!TOKEN_SHAPE.test(token)) return "That is not a hummingbird token (hb_ and 64 hex characters, 67 in all).";
  return null;
}

const MISROUTED = {
  kind: "misrouted",
  message: "The authority answered with a page, not the API. Nothing was saved.",
};

/// What a `POST /api/items` response means. `kind` drives the popup;
/// `message` is what it says. `saved` is the only success.
export function classify({ status, contentType, bodyText, id }) {
  const type = (contentType ?? "").toLowerCase();
  const isJson = type.startsWith("application/json");
  const isHtml = type.startsWith("text/html");
  if (status === 201 || status === 200) {
    if (!isJson) return MISROUTED;
    let row;
    try {
      row = JSON.parse(bodyText);
    } catch {
      return MISROUTED;
    }
    if (row?.id !== id) return MISROUTED;
    // 200 is the idempotent replay of an id some popup already sent — the
    // row is the stored one, and the popup checks it against what it sent
    // (`rowMatches`). It names the destination itself on a fresh 201.
    return { kind: "saved", replay: status === 200, row, message: status === 201 ? "Saved" : "Already saved" };
  }
  if (status === 401) {
    return { kind: "token", message: "The token was rejected. Sign in again in the extension's options." };
  }
  if (status === 403) {
    // The authority's 403 is an empty body; Cloudflare's is an HTML page
    // (`error code: 1010`, #326). Only the body tells them apart.
    if (isHtml) return { kind: "edge", message: "Blocked at the edge (Cloudflare), not by the authority. Nothing was saved." };
    return { kind: "scope", message: "This token cannot create items. Mint a device token, not another scope." };
  }
  if (status === 400) {
    let message = "The authority refused the item.";
    if (isJson) {
      try {
        const detail = JSON.parse(bodyText)?.message;
        if (typeof detail === "string" && detail !== "") message = `The authority refused the item: ${detail}`;
      } catch {
        // The generic message stands.
      }
    }
    return { kind: "rejected", message };
  }
  return { kind: "error", message: `The authority answered ${status}. Save again to retry.` };
}

// ---------------------------------------------------------------------------
// Sign in with Google (ADR-0040 decision 3, the extension leg). The options
// page opens `authUrl` through `chrome.identity.launchWebAuthFlow` — an
// implicit `response_type=id_token` request, because `getAuthToken` yields
// only access tokens — reads the ID token off the redirect's fragment, and
// trades it at `POST /api/auth/google` for an ordinary `device` token, which
// it stores exactly where a pasted one goes. The nonce is checked here
// (`parseSigninRedirect`) so an answer meant for another request is never
// sent; everything else about the ID token — signature, audience, `sub`,
// freshness — is the authority's to verify, and nothing here tries.
//
// `signinOutcome` is the core's `sign_in_outcome`
// (`client/core/src/devices.rs`) re-stated, with `classify`'s
// content-type-first guard in front for the same reason `classify` has one.

/// The Google OAuth Web client every surface signs in through — public by
/// design, and one of the authority's accepted audiences
/// (`GOOGLE_SIGNIN_CLIENT_IDS` in `server/worker/wrangler.toml`;
/// `signin.test.js` pins that it still is). Its authorized redirect URIs
/// must list this extension's `https://<id>.chromiumapp.org/`.
export const GOOGLE_SIGNIN_CLIENT_ID = "309477759674-d169knsap6cuig582ii1mhv032u3ovb2.apps.googleusercontent.com";

const GOOGLE_AUTH_ENDPOINT = "https://accounts.google.com/o/oauth2/v2/auth";

/// The sign-in route: unauthenticated, the ID token is the credential.
export const SIGNIN_PATH = "/api/auth/google";

/// A fresh nonce: 16 random bytes as hex. `bytes` is for tests.
export function newNonce(bytes = crypto.getRandomValues(new Uint8Array(16))) {
  return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/// The URL `launchWebAuthFlow` opens. `redirectUri` is
/// `identity.getRedirectURL()`: `https://<extension-id>.chromiumapp.org/`,
/// stable because the manifest carries a `key`.
export function authUrl({ redirectUri, nonce, clientId = GOOGLE_SIGNIN_CLIENT_ID }) {
  const params = new URLSearchParams({
    client_id: clientId,
    response_type: "id_token",
    scope: "openid email",
    redirect_uri: redirectUri,
    nonce,
  });
  return `${GOOGLE_AUTH_ENDPOINT}?${params}`;
}

/// A JWT's payload, or `null` when it is not three segments around a
/// base64url JSON object. The signature is not checked: that is the
/// authority's job.
function jwtClaims(jwt) {
  const parts = jwt.split(".");
  if (parts.length !== 3) return null;
  try {
    const b64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const binary = atob(b64.padEnd(Math.ceil(b64.length / 4) * 4, "="));
    const claims = JSON.parse(new TextDecoder().decode(Uint8Array.from(binary, (c) => c.charCodeAt(0))));
    return claims !== null && typeof claims === "object" ? claims : null;
  } catch {
    return null;
  }
}

/// The ID token from the URL `launchWebAuthFlow` resolved with, or the
/// reason there is none. An `error` in the fragment is Google's refusal
/// (`access_denied` is the person closing the consent); a token whose
/// `nonce` is not this request's is refused unsent.
export function parseSigninRedirect(redirectedTo, nonce) {
  let fragment;
  try {
    fragment = new URLSearchParams(new URL(redirectedTo).hash.slice(1));
  } catch {
    return { problem: "Google's answer carried no ID token." };
  }
  const error = fragment.get("error");
  if (error === "access_denied") return { problem: "Sign-in was cancelled." };
  if (error) {
    const detail = fragment.get("error_description");
    return { problem: `Google refused the sign-in: ${error}${detail ? ` (${detail})` : ""}.` };
  }
  const idToken = fragment.get("id_token");
  if (!idToken) return { problem: "Google's answer carried no ID token." };
  const claims = jwtClaims(idToken);
  if (claims === null) return { problem: "Google's ID token could not be read." };
  if (typeof nonce !== "string" || nonce === "" || claims.nonce !== nonce) {
    return { problem: "Google's answer is not for this sign-in. Sign in again." };
  }
  return { idToken };
}

/// The name a sign-in offers by default: the machine this profile runs on,
/// as far as an extension can know it — `runtime.getPlatformInfo().os`; a
/// hostname is out of its reach. Editable before signing in; the authority
/// derives the token id from it (`device-chrome-<slug>`).
export function defaultDeviceName(os) {
  const names = { mac: "Mac", win: "Windows PC", linux: "Linux", cros: "Chromebook" };
  return names[os] ?? "Browser";
}

/// `GoogleSignIn` (`server/domain/src/api.rs`, `deny_unknown_fields`):
/// exactly these three keys.
export function signinBody({ idToken, name }) {
  return { id_token: idToken, name: name.trim(), platform: "chrome" };
}

/// What a `POST /api/auth/google` response means. `signed_in` carries the
/// token to store; every other kind is a message and nothing stored.
export function signinOutcome({ status, contentType, bodyText }) {
  const type = (contentType ?? "").toLowerCase();
  const isJson = type.startsWith("application/json");
  const body = () => {
    try {
      return isJson ? JSON.parse(bodyText) : null;
    } catch {
      return null;
    }
  };
  if (status === 201 || status === 200) {
    const minted = body();
    if (status !== 201 || typeof minted?.id !== "string" || tokenProblem(minted.token) !== null) return MISROUTED;
    return { kind: "signed_in", id: minted.id, token: minted.token, message: `Signed in as ${minted.id}.` };
  }
  if (status === 403) {
    if (type.startsWith("text/html")) return { kind: "edge", message: "Blocked at the edge (Cloudflare), not by the authority. Nothing was stored." };
    return { kind: "not_allowed", message: "The authority refused that Google account: it is not the one allowed to sign in." };
  }
  if (status === 429) {
    return { kind: "rate_limited", message: "Too many sign-ins in the last hour. Try again later, or paste a token." };
  }
  if (status === 503) {
    if (body()?.error === "signin_unconfigured") {
      return { kind: "unconfigured", message: "Sign-in is not set up on the authority. Paste a token instead." };
    }
    return { kind: "unavailable", message: "The authority could not check the sign-in with Google. Try again shortly." };
  }
  if (status === 400) {
    const detail = body()?.message;
    return {
      kind: "invalid",
      message: typeof detail === "string" && detail !== "" ? `The authority refused the device name: ${detail}` : "The authority refused the device name.",
    };
  }
  return { kind: "error", message: `The authority answered ${status}. Nothing was stored.` };
}
