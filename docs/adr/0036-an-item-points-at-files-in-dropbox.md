# ADR-0036: An item points at files in Dropbox — paths a machine-local helper opens

**Status:** accepted · 2026-09-08
**Context:** the file-links grilling of 2026-09-03/08. #771 gave an item one
pointer at a note in the Obsidian vault (`items.vault_path`, ADR-0009's own
amendment). The operator wanted the same gesture for the *things* a task is
about — a receipt, a scanned letter, the PDF the work is on — which live in
Dropbox, and wanted them to open in the native file browser on each
machine. Amends [ADR-0009](0009-the-owned-schema-and-context-lanes.md) (a
table joins the synced set) and
[ADR-0002](0002-sources-join-by-role-urgency-computed-at-read-time.md) (a
second one-way pointer out of the app that does not make its far side a
source). [ADR-0020](0020-no-delete-rows-are-flagged-not-erased.md) governs
removal, unchanged.

## The decision

1. **Many per item, as a table.** `file_links` (`id`, `item_id`, `path`,
   `removed_at`, `version`) joins the synced set and rides the version-
   counter delta like every other table. No `label`: the basename is the
   name. No `position`: rows list in insertion order, and a table nobody
   can rearrange needs no column saying how it is arranged. A scalar beside
   `vault_path` was rejected because a task is about several files and a
   one-file rule would push the rest into the note, which is where the
   *thinking* goes, not the evidence.

   ```sql
   CREATE TABLE IF NOT EXISTS file_links (
     id          TEXT PRIMARY KEY,
     item_id     TEXT NOT NULL REFERENCES items(id),
     path        TEXT NOT NULL,   -- Dropbox-relative, e.g. 'Finance/2026/receipt.pdf'
     removed_at  INTEGER,         -- flagged, never erased (ADR-0020)
     version     INTEGER NOT NULL
   )
   ```

   `SCHEMA_VERSION` 14 → 15, purely additive (14 was taken by #782's `items.link_url`/`link_label` while this slice was in flight). It references `items`, so it
   joins `FK_CHILDREN` — the tables an `items` rebuild stands aside — where
   `project_links` (which references `projects`) does not.

2. **The term is File link, never "attachment".** An attachment is already
   the Grill's immutable, system-written record (ADR-0023 decision 2), and
   the glossary's Vault path entry leans on that contrast. A file link is
   a pointer the human chose.

3. **The stored value is a Dropbox-relative path, never a URL.** The Vault
   path principle exactly: a path is the domain fact and survives the
   Dropbox folder living at `C:\Dropbox` on one machine and
   `~/Library/CloudStorage/Dropbox` on another; a URL is transport and goes
   stale. The authority validates non-blank-after-trim and nothing more;
   every shape rule (no leading `/`, no `..`, no drive letter, no `~`) is
   client-side vendor knowledge in `client/web/src/dropbox/file-link.ts`,
   applied to stored values as well as typed ones — a path that fails
   draws no Open button, which is how the client refuses.

4. **No binding for the root.** Where Dropbox lives on a machine is a fact
   about that machine, and a `settings` row is synced to every machine —
   so a binding would be wrong everywhere but one. The root lives in the
   helper (decision 5) on each machine. The web app additionally keeps a
   **device-local** copy in `localStorage`, entered once in Settings, for
   one purpose: stripping it off a pasted "Copy as path" clipboard. That
   copy shapes text and is never sent anywhere; it is the kind of thing
   the glossary already lets a View own.

5. **Opening fires a custom URL scheme handled by a once-per-machine
   helper.** A page served over https cannot navigate to `file://`; no OS
   ships a scheme that opens Explorer or Finder at a path; Dropbox's
   desktop app publishes none. So the app fires
   `hummingbird-open:?path=<encoded path>` — a scheme this repo owns,
   spelled with no `//` so no parser reads the first segment as a host —
   and `tools/dropbox-open/` claims it: a per-user registry entry plus a
   PowerShell handler on Windows, an `osacompile` applet plus a bash
   resolver on the Mac. The helper maps the path onto its root and opens
   the result in its default app; a folder path opens the file browser at
   that folder. **Open, not reveal**, by the operator's choice. This is the
   `obsidian://` mechanism, self-supplied.

   *Amended 2026-09-08 (the wrap-up review of this slice): executable file
   types are the one exception to "open". A `.exe`, `.bat`, `.cmd`, `.lnk`
   or `.ps1` on Windows, a `.app`, `.command`, `.sh` or `.pkg` on the Mac
   (each helper carries its own list), is **revealed** in the file browser
   with the file selected, never launched — because "open in default app"
   on those is "run", and a URL any web page can fire after one prompt
   click must not run whatever the operator happens to keep in Dropbox.
   Everything else still opens.*

6. **The dropbox.com fallback is always drawn.** The app cannot tell
   whether the helper is installed, so beside every Open is an "on
   dropbox.com" link to the same path in Dropbox's web UI
   (`/home/<folder>?preview=<basename>`). No state, and no machine ever
   looks broken. On Android that URL lands in the Dropbox app, which is
   what the phone will use when it draws this list (its own issue; in this
   slice the phone carries the table and draws nothing, on #771's
   precedent).

7. **Add and remove only.** `POST /api/file_links` creates; `PATCH
   /api/file_links/:id` carries `removed_at` alone (set, or `null` to
   un-remove) under CAS, and refuses a `path` key outright. Re-pointing is
   remove-then-add. This halves the handler and Core surface, and a link
   is cheap to re-add.

## Consequences

- One more thing to install per machine, like Obsidian itself. The README
  in `tools/dropbox-open/` is the install and the manual test; neither
  half is in CI, because a URL-scheme handler is a registration against a
  live desktop session with no artefact to build.
- The scheme name is a contract between three places (the web constant
  and both registrations). Renaming it is never a silent change.
- The fallback depends on being signed in to dropbox.com in that browser.
- Every device discards its mirror snapshot and resweeps once
  (`SYNC_MIRROR_SCHEMA_VERSION` 5 → 6): a new table is the bump case.
- The `localStorage` root is per browser profile and dies with site data.
  Acceptable, because it only ever shapes a paste; the strict field still
  works without it.

## Alternatives rejected

- **Dropbox's web UI as the primary gesture.** Zero setup and works
  everywhere, but it leaves the native file browser out, which was the
  whole ask. It survives as the fallback.
- **The File System Access API.** An installed PWA can hold a persistent
  directory handle on the Dropbox root and open a file's bytes in a new
  tab. It never opens Explorer or Finder, and Office files only download.
- **A share URL per file.** Hand-minted in Dropbox's UI, revocable, and
  transport in the store.
- **A synced binding for the root.** Decision 4.
- **A fourth field on `ItemPatch`.** A scalar; decision 1.
