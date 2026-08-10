// The device token's resting place (ADR-0004, amended by ADR-0008/#106):
// IndexedDB — "the only option in a browser" per that ADR's table — and
// deliberately a *different* database from the core's own snapshot store
// (`hummingbird-task`, core.worker.ts's `TASK_NAMESPACE`). The core never
// persists the key it is handed (see `client/core/src/lib.rs`'s
// `compile_fail` proof on `Core`); this module is the host-owned exception
// ADR-0004 carves out, kept in its own namespace so a mirror export can
// never carry it.
//
// `TaskTokenStoreLike` is the injectable seam: `token.ts`'s logic is
// unit-tested against an in-memory fake, and `createIndexedDbTaskTokenStore`
// below is the real, browser-only implementation — the same split
// `google/gis.ts` uses for its own untestable environment glue.

/** One stored device token. `enteredAtMs` is metadata only (rendered via the
 * mono meta style, per the design system) — never logged or displayed as
 * part of an error, and never the token's own value. */
export interface TaskTokenRecord {
  token: string;
  enteredAtMs: number;
}

export interface TaskTokenStoreLike {
  read(): Promise<TaskTokenRecord | null>;
  write(record: TaskTokenRecord): Promise<void>;
  clear(): Promise<void>;
}

const DB_NAME = "hummingbird-task-token";
const DB_VERSION = 1;
const STORE_NAME = "token";
const RECORD_KEY = "device-token";

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      request.result.createObjectStore(STORE_NAME);
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("failed to open indexeddb"));
  });
}

/** The real, browser-only `TaskTokenStoreLike` — untested here for the same
 * reason `google/gis.ts`'s `createGisTokenClient` is: it is thin glue over
 * a browser API vitest's node environment does not have. The logic that
 * decides what to do with what it reads (`token.ts`) is what carries the
 * test coverage. */
export function createIndexedDbTaskTokenStore(): TaskTokenStoreLike {
  return {
    async read() {
      const db = await openDb();
      try {
        return await new Promise<TaskTokenRecord | null>((resolve, reject) => {
          const tx = db.transaction(STORE_NAME, "readonly");
          const request = tx.objectStore(STORE_NAME).get(RECORD_KEY);
          request.onsuccess = () => resolve((request.result as TaskTokenRecord | undefined) ?? null);
          request.onerror = () => reject(request.error ?? new Error("failed to read device token"));
        });
      } finally {
        db.close();
      }
    },
    async write(record) {
      const db = await openDb();
      try {
        await new Promise<void>((resolve, reject) => {
          const tx = db.transaction(STORE_NAME, "readwrite");
          tx.objectStore(STORE_NAME).put(record, RECORD_KEY);
          tx.oncomplete = () => resolve();
          tx.onerror = () => reject(tx.error ?? new Error("failed to write device token"));
        });
      } finally {
        db.close();
      }
    },
    async clear() {
      const db = await openDb();
      try {
        await new Promise<void>((resolve, reject) => {
          const tx = db.transaction(STORE_NAME, "readwrite");
          tx.objectStore(STORE_NAME).delete(RECORD_KEY);
          tx.oncomplete = () => resolve();
          tx.onerror = () => reject(tx.error ?? new Error("failed to clear device token"));
        });
      } finally {
        db.close();
      }
    },
  };
}
