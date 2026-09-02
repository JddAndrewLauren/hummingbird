// #771's whole store of Obsidian vendor knowledge: how a title becomes a
// vault-relative path, how a path becomes the one URI that opens or creates
// that note, and what shapes of path this client refuses to send.
//
// **Why any of this is client-side.** What the authority stores is a *path*
// (`items.vault_path`) — a domain fact about where an item's thinking lives.
// A URI is transport, and it is transport for exactly one vendor's desktop
// app on exactly one kind of device. Putting the scheme, the parameter names
// and the escaping rules in the core (or worse, in the schema) would make
// every other client carry Obsidian's spelling to reach a column that has
// nothing to do with it, and would put a value in the store that a vault
// rename invalidates. Storing the path instead is also what makes the values
// a direct import key for the owned notes lane #192 will hold.
//
// **`&append` is load-bearing and one token wide.** Verified empirically
// against a real vault (2026-09-02): with the flag, firing at an existing
// note opens it and preserves its content byte-for-byte; *without* it,
// Obsidian silently creates `<name> 1.md` alongside the original and the
// item ends up pointing at a note it never wrote — a data-integrity bug
// invisible until the notes have doubled. There is a forum report that
// `append` does nothing; it does not describe this build. `buildUri` has a
// unit test asserting the flag survives, and that test is not ceremony.
//
// **The vault is named, never identified.** `obsidian.json` registers each
// vault under a per-machine generated hex id, so the same folder on two of
// the operator's machines carries two different ids. The name is one fact,
// which is why the `obsidian-vault` binding holds a name.

import type { BindingDTO } from "../store/protocol";

/** ADR-0015's binding key for the Obsidian vault name. Resolved by name here
 * the same way `calendar/selection.ts` resolves its own — the vocabulary
 * itself lives in `hummingbird_core::bindings`, and `Core::bindings` is what
 * says which keys exist. */
export const OBSIDIAN_VAULT_BINDING_KEY = "obsidian-vault";

/** The folder every derived path lands in. One folder, so the whole of what
 * this feature ever wrote can be removed by deleting it. */
const FOLDER = "Hummingbird";

/** Characters Obsidian will not accept in a note's file name. Stripped
 * rather than substituted: a title is a human sentence, and a `?` turning
 * into a `_` reads as a typo in the file listing where dropping it reads as
 * nothing at all. */
const FORBIDDEN = /[*"\\/<>:|?]/g;

/** The name of the operator's vault, or `null` when there isn't one.
 *
 * Four inputs collapse to `null` deliberately — an unread bindings table, an
 * unset row, a row holding something that is not text, and a row blanked to
 * whitespace (the nearest thing `settings` has to a DELETE) — exactly as
 * `tripsCalendarId` collapses its own four. None of them names a vault, and
 * the affordance is simply not drawn for any of them. */
export function obsidianVaultName(bindings: BindingDTO[] | null): string | null {
  if (bindings === null) {
    return null;
  }
  const binding = bindings.find((candidate) => candidate.key === OBSIDIAN_VAULT_BINDING_KEY);
  if (binding === undefined || binding.value.state !== "text") {
    return null;
  }
  const name = binding.value.text.trim();
  return name === "" ? null : name;
}

/** The path a "Start a note" gesture proposes for `title`:
 * `Hummingbird/<title with the forbidden characters stripped>.md`, or `null`
 * when there is no path to propose.
 *
 * Deterministic and reversible — the same title always proposes the same
 * path, so a second click on an item whose path was cleared re-points it at
 * the note it had. It is only ever a *proposal*: the path is a stored value
 * the operator can edit afterwards, and nothing here ever renames a note to
 * follow a retitled item.
 *
 * **A title that strips to nothing has no proposal.** `???` is a title the
 * form accepts (it is not blank), and stripping leaves an empty basename —
 * `Hummingbird/.md`, a hidden file every such item would then share. The
 * answer is `null` rather than an invented `Untitled`: a fallback name is
 * one the operator never wrote, persisted by the very click that opens it,
 * and it breaks the determinism above the moment two of them meet. The
 * caller drops the affordance instead. */
export function derivePath(title: string): string | null {
  const name = title.replace(FORBIDDEN, "").trim();
  return name === "" ? null : `${FOLDER}/${name}.md`;
}

/** The one URI this feature ever fires: `obsidian://new`, which opens the
 * note at `path` in `vault` and creates it (along with any intermediate
 * folder — so `Hummingbird/` needs no setup) when it is not there.
 *
 * `encodeURIComponent` on both values, per Obsidian's own documentation: the
 * `file` parameter needs `/` as `%2F` and a space as `%20`, which is exactly
 * what it produces and what a hand-rolled escape gets wrong.
 *
 * The trailing `&append` is what makes the call idempotent — see this
 * module's header. It is a valueless flag, so it is spelled as one. */
export function buildUri(vault: string, path: string): string {
  const params = `vault=${encodeURIComponent(vault)}&file=${encodeURIComponent(path)}`;
  return `obsidian://new?${params}&append`;
}

/** Whether `path` is something this client will send.
 *
 * Three rules, and deliberately no fourth. Non-empty after trim is the same
 * one the authority enforces. A leading `/` and a `..` segment are both
 * attempts to leave the vault, which `obsidian://new` would either refuse or
 * resolve somewhere nobody meant.
 *
 * **There is no `.md` requirement.** The `file` parameter allows the
 * extension to be omitted entirely, and the vault also holds `.canvas` and
 * `.base` files — a rule demanding `.md` would reject paths that work. */
export function isValidVaultPath(path: string): boolean {
  const trimmed = path.trim();
  if (trimmed.length === 0) {
    return false;
  }
  if (trimmed.startsWith("/")) {
    return false;
  }
  return !trimmed.split("/").includes("..");
}
