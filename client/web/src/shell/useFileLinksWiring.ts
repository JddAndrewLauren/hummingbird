import type { FileLinkDTO } from "../store/protocol";
import type { WorkerLike } from "../store/worker-client";
import { createFileLink, removeFileLink } from "../store/worker-client";

// ADR-0036's file-links wiring: the item panel's two writes, plus the one
// device-local fact the panel needs to shape a paste. Travels to `ItemPanel`
// as one object prop the way `microtask` does (`App.tsx` → `NowScreen` /
// `ProjectsScreen` → `FrontierBoard` → `ItemPanel`).
//
// **It deliberately owns no refresher.** `useItemDetailWiring.ts` requests
// the open item's file links beside its steps, on the same `syncOutcomeSeq`
// key, so a second per-cycle requester here would be a second clock for one
// read (CLAUDE.md's "No competing clocks"). Write door only.
//
// Nothing here is optimistic: `createFileLink` enqueues, and the link
// appears in `TaskState.fileLinksByItem` only when a completed cycle pulls
// it back (`Core::create_file_link`'s own doc). Every write RETURNS the seed
// it minted so the panel can scope `lastFileLinkWrite` — one broadcast slot
// shared by every open panel — to its own outstanding write, the
// `useProjectsWiring` contract.

export interface FileLinksWiring {
  /** This device's Dropbox folder, or `null` when Settings has none —
   * `normalizePastedPath`'s second argument, nothing more. */
  localRoot: string | null;
  /** `path` must already be normalized and shape-checked
   * (`dropbox/file-link.ts`); this door does not judge it. */
  createFileLink: (itemId: string, path: string) => string;
  removeFileLink: (current: FileLinkDTO) => string;
}

/** Non-deterministic, same "creates a new entity" reasoning as
 * `mintProjectLinkCreateSeed`. */
export function mintFileLinkCreateSeed(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `file-link-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

/** Deterministic, same `mintProjectLinkPatchSeed` reasoning: retrying the
 * identical removal must reproduce the identical queue entry. */
export function mintFileLinkRemoveSeed(linkId: string, nowMs: number): string {
  return `${linkId}:remove:${nowMs}`;
}

export function useFileLinksWiring(worker: WorkerLike, localRoot: string | null): FileLinksWiring {
  return {
    localRoot,
    createFileLink: (itemId, path) => {
      const seed = mintFileLinkCreateSeed();
      createFileLink(worker, seed, itemId, path, Date.now());
      return seed;
    },
    removeFileLink: (current) => {
      const nowMs = Date.now();
      const seed = mintFileLinkRemoveSeed(current.id, nowMs);
      removeFileLink(worker, seed, current, nowMs, nowMs);
      return seed;
    },
  };
}
