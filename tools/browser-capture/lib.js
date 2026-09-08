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
  return host === "" ? null : host.toLowerCase();
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
    // 200 is the idempotent replay of an id this popup already sent; the
    // popup names the destination itself on a fresh 201.
    return { kind: "saved", replay: status === 200, message: status === 201 ? "Saved" : "Already saved" };
  }
  if (status === 401) {
    return { kind: "token", message: "The token was rejected. Paste a fresh one in the extension's options." };
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
