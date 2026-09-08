// ADR-0036's whole store of Dropbox vendor knowledge: how a pasted local
// path becomes a Dropbox-relative one, how a path becomes the one scheme
// this client fires, how the same path becomes the dropbox.com fallback, and
// what shapes of path this client refuses to send.
//
// **Why any of this is client-side.** What the authority stores is a *path*
// (`file_links.path`) — a domain fact about which file an item points at,
// relative to the operator's Dropbox. Everything here is transport: a custom
// scheme for one kind of device, a web URL for another, and a paste rule for
// a specific file manager's clipboard. None of it belongs in the core or the
// schema, for exactly the reason `obsidian/vault-uri.ts` gives.
//
// **The scheme is `hummingbird-open:`, and it has no `//`.** An https page
// cannot navigate to `file://`, no OS ships a scheme that opens Explorer or
// Finder at a path, and Dropbox's desktop app publishes none either — so the
// scheme is this repo's own, claimed by the once-per-machine helper in
// `tools/dropbox-open/`. It is spelled `hummingbird-open:?path=…` rather than
// `hummingbird-open://…` because a `//` form makes the first path segment a
// host, which some parsers lower-case and some reject; a bare query string
// survives every parser the same way. The helper is where the machine-local
// Dropbox root lives (`C:\Dropbox`, `~/Library/CloudStorage/Dropbox`), which
// is why the app holds no binding for it.
//
// **The local root the *browser* knows is a paste convenience, not a fact.**
// "Copy as path" hands over `"C:\Dropbox\Finance\2026\receipt.pdf"` — quoted,
// back-slashed, absolute. `normalizePastedPath` turns that into
// `Finance/2026/receipt.pdf` when this device has told Settings where its
// Dropbox folder is (`local-root.ts`, localStorage, never synced). A device
// that has not simply gets the strict field.

/** The scheme the helper claims. Renaming it is a two-place change (this
 * constant and both helpers' registrations); never do it silently. */
export const OPEN_SCHEME = "hummingbird-open";

/** The one URI the Open button ever fires. `encodeURIComponent` so `/` is
 * `%2F` and a space `%20`, which both helpers decode the same way. */
export function buildOpenUri(path: string): string {
  return `${OPEN_SCHEME}:?path=${encodeURIComponent(path)}`;
}

/** The fallback: the same file in Dropbox's own web UI, for a device with
 * no helper (or a phone, where dropbox.com links land in the Dropbox app).
 *
 * A file previews at `/home/<folder>?preview=<basename>`. A folder has no
 * preview, so a path whose last segment carries no `.` is linked as the
 * folder itself — a heuristic, and a named one: `Reports/2026.q1` would be
 * read as a file. Live with it; the Open button is the primary gesture. */
export function buildDropboxWebUrl(path: string): string {
  const segments = path.split("/").filter((segment) => segment.length > 0);
  const last = segments[segments.length - 1] ?? "";
  const encode = (parts: string[]) => parts.map(encodeURIComponent).join("/");
  if (!last.includes(".")) {
    return `https://www.dropbox.com/home/${encode(segments)}`;
  }
  const folder = encode(segments.slice(0, -1));
  return `https://www.dropbox.com/home/${folder}?preview=${encodeURIComponent(last)}`;
}

/** What one file manager's clipboard turns into: trims, strips one pair of
 * surrounding quotes, turns `\` into `/`, strips `localRoot` when the paste
 * starts with it (case-insensitively, with or without a trailing slash), and
 * drops the leading `/` that cut leaves behind. Returns a candidate only —
 * `isValidFilePath` is the judge, and a paste from the wrong machine that
 * still starts with a drive letter fails there rather than here. */
export function normalizePastedPath(raw: string, localRoot: string | null): string {
  let path = raw.trim();
  if (path.length >= 2 && ((path.startsWith('"') && path.endsWith('"')) || (path.startsWith("'") && path.endsWith("'")))) {
    path = path.slice(1, -1).trim();
  }
  path = path.replace(/\\/g, "/");
  if (localRoot !== null) {
    const root = localRoot.trim().replace(/\\/g, "/").replace(/\/+$/, "");
    if (root.length > 0 && path.toLowerCase().startsWith(root.toLowerCase())) {
      const rest = path.slice(root.length);
      if (rest === "" || rest.startsWith("/")) {
        path = rest.replace(/^\/+/, "");
      }
    }
  }
  return path;
}

/** `\` read as `/`. The authority stores whatever it was sent, and a row
 * written by some other client (or a paste that skipped `normalizePastedPath`)
 * may carry back-slashes; every reader here — the judge, the two URL
 * builders, the row's name split — sees the path through this first, so a
 * `\` can never smuggle a segment past a `/`-only split. */
export function normalizeSeparators(path: string): string {
  return path.replace(/\\/g, "/");
}

/** Whether `path` is something this client will send, or will draw Open for.
 *
 * Non-empty after trim is the one rule the authority shares. The rest are
 * attempts to leave Dropbox, or a paste that never became relative: a
 * leading `/` (or `\`), a `..` segment, a drive letter, a `~`. A `.`
 * segment or an empty one (`a//b`, a trailing `/`) is refused too — not an
 * escape, but a path no helper resolves the same way twice. Judged on
 * separator-normalized form, so `a\..\b` fails exactly as `a/../b` does.
 * No extension rule — a folder is a legitimate link, and opening one opens
 * the file browser. */
export function isValidFilePath(path: string): boolean {
  const trimmed = normalizeSeparators(path).trim();
  if (trimmed.length === 0) {
    return false;
  }
  if (trimmed.startsWith("/") || trimmed.startsWith("~")) {
    return false;
  }
  if (/^[A-Za-z]:/.test(trimmed)) {
    return false;
  }
  return trimmed.split("/").every((segment) => segment !== "" && segment !== "." && segment !== "..");
}

/** The name a row shows: the basename, with the folder as secondary text.
 * A path with no `/` is its own basename and has no folder. */
export function splitFilePath(path: string): { name: string; folder: string | null } {
  const index = path.lastIndexOf("/");
  if (index < 0) {
    return { name: path, folder: null };
  }
  return { name: path.slice(index + 1), folder: path.slice(0, index) };
}
