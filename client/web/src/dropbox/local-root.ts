// Where this device's Dropbox folder is (ADR-0036), persisted device-locally
// on the `skills/backend-selection.ts` pattern: a narrow `StorageLike`, never
// the sync engine. It is a paste convenience — `file-link.ts`'s
// `normalizePastedPath` strips it off a "Copy as path" clipboard — and
// deliberately **not a binding**: `C:\Dropbox` on the PC and
// `~/Library/CloudStorage/Dropbox` on the Mac are facts about machines, and
// a synced value would be wrong on every machine but one. The helper in
// `tools/dropbox-open/` holds its own copy for the open gesture; the two
// never need to agree, because this one only ever shapes text.

const LOCAL_ROOT_KEY = "hb.dropbox-local-root";

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/** `null` when nothing is stored, nothing is storable, or the stored value
 * is blank — every one of those means "no root known on this device". */
export function readDropboxLocalRoot(storage: StorageLike | undefined): string | null {
  if (!storage) return null;
  const raw = storage.getItem(LOCAL_ROOT_KEY);
  if (raw === null) return null;
  const trimmed = raw.trim();
  return trimmed === "" ? null : trimmed;
}

/** A blank value removes the key rather than storing an empty string, so
 * "cleared" and "never set" read the same way. */
export function writeDropboxLocalRoot(storage: StorageLike | undefined, value: string): void {
  if (!storage) return;
  const trimmed = value.trim();
  if (trimmed === "") {
    storage.removeItem(LOCAL_ROOT_KEY);
    return;
  }
  storage.setItem(LOCAL_ROOT_KEY, trimmed);
}
