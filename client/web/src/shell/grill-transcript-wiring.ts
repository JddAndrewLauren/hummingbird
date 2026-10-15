import { createGrillTranscriptClient, type FetchTranscript } from "../grill/transcript-client";
import { createIndexedDbTaskTokenStore } from "../task/token-store";

// #358: the one `FetchTranscript` the app's Grill histories share, built
// lazily on first use exactly the way `useCalendarWiring.ts`'s `tokenClient`
// is — the IndexedDB token store is opened only once a reader actually
// expands a transcript, never at render.

let cached: FetchTranscript | null = null;

export const fetchGrillTranscript: FetchTranscript = (grillId) => {
  if (cached === null) {
    const tokenStore = createIndexedDbTaskTokenStore();
    cached = createGrillTranscriptClient({
      fetch: globalThis.fetch.bind(globalThis),
      readToken: async () => (await tokenStore.read())?.token ?? null,
    });
  }
  return cached(grillId);
};
