/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** The Google OAuth client id GIS consent (issue #73) requests
   * `calendar.readonly` against — the sweeper's Workspace Internal OAuth
   * client id may be reused here (an independent per-device credential;
   * its refresh token and Tasks/Gmail scopes are untouched). Unset in dev
   * by default: the calendar opt-in UI simply doesn't render without it. */
  readonly VITE_GOOGLE_CLIENT_ID?: string;
  /** The owned authority's origin (ADR-0003/0008) the task binding's
   * `TaskHost::runSync` sends `Core::run`'s two `reqwest` transports
   * against (#105/S7) — `core` invents no deployment address of its own.
   * Unset in dev by default: `core.worker.ts` falls back to `""`, which
   * every `runSync` call reports as a fast, network-free `"pull_failed"`
   * rather than doing nothing. #106/S8 is what actually surfaces a device
   * token entry flow; until then this only gates whether `runSync` can ever
   * reach a real server. */
  readonly VITE_API_BASE_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
