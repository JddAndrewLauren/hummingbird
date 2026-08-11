// Issue #172: the id every view's `ready` handshake carries, minted once per
// `SharedWorker` global scope (`core.worker.ts` evaluates exactly once per
// core, so its one call IS the core instance). ADR-0010's probe: two views
// showing the same id share one core, two different ids refute it.
//
// **This is a diagnostic, and a diagnostic may never take the core down** —
// which is the whole reason it is a module of its own rather than an inline
// expression at `core.worker.ts`'s module scope. `crypto.randomUUID` is
// **secure-context only** (https or localhost), so serving the app over a
// LAN IP for a device test — `pnpm dev --host` on `http://192.168.x.x:5173`,
// not a secure context — leaves it `undefined`. Called bare, that throws
// during module evaluation, BEFORE `self.onconnect` is assigned; and a
// `connect` event has no platform buffering, so every view hangs on
// "Loading core…" forever. That is the exact failure `core.worker.ts`'s own
// module doc is written to prevent, arriving through an eight-character
// string nothing depends on.
//
// The guard is `shell/useCaptureWiring.ts`'s `mintSeed` idiom, deliberately
// unchanged: this repo already answers "what if there is no
// `crypto.randomUUID`" in two places, and a third answer would be a third
// thing to keep in step.
//
// Uniqueness here is far weaker than `mintSeed`'s: the only ids that must
// differ are those of cores alive **on one device at one moment**, and they
// are compared by eye, never keyed on or persisted. A collision costs a
// misread probe, not a lost write.

/** Eight hex-ish characters — enough to tell two live cores apart at a
 * glance, short enough to sit in one line of an aside card. */
const ID_LENGTH = 8;

/** `source` is the `crypto` to read, injected so a test can exercise the
 * fallback arm without stubbing a global (`mintSeed` predates this and stubs
 * one; both are fine, and this one is a plain argument because there is no
 * hook or component in the way). Defaults to the ambient `crypto`, which is
 * what `core.worker.ts` calls it with. */
export function mintCoreId(source: Crypto | undefined = globalThis.crypto): string {
  if (source !== undefined && "randomUUID" in source) {
    return source.randomUUID().slice(0, ID_LENGTH);
  }
  // No secure context. Not an error and not a blank: an id that says
  // "minted without crypto" is still a perfectly good probe — it is compared
  // against another window's, never parsed — and rendering nothing here
  // would read as "this build has no diagnostic".
  return Math.random().toString(16).slice(2, 2 + ID_LENGTH).padEnd(ID_LENGTH, "0");
}
