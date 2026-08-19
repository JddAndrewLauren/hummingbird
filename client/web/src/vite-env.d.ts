/// <reference types="vite/client" />
/// <reference types="vite-plugin-pwa/client" />

// `__APP_VERSION__` — `vite.config.ts`'s build-version `define` — is
// deliberately NOT declared here: `shell/build-version.ts` is compiled by
// the node project too (it feeds `build-version.node.ts`), which does not
// include this file, so the declaration lives in that module instead.

interface ImportMetaEnv {
  /** The Google OAuth client id issue #73 introduced — the sweeper's
   * Workspace Internal OAuth client id may be reused here (an independent
   * per-device credential; its refresh token and Tasks/Gmail scopes are
   * untouched). Unset in dev by default: the calendar opt-in UI simply
   * doesn't render without it. #583/#584 moved every token request — silent
   * re-mint, rotation, and the interactive Connect/Reconnect press — onto
   * the authority (`calendar/authority-token-client.ts`), which authenticates
   * with the device token and never reads this id at all. What remains of
   * this gate is the Settings section's rendering condition
   * (`shell/useCalendarWiring.ts`'s `GOOGLE_CLIENT_ID` export); whether that
   * is still the right gate is #585's question. */
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
