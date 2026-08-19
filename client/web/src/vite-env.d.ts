/// <reference types="vite/client" />
/// <reference types="vite-plugin-pwa/client" />

// `__APP_VERSION__` — `vite.config.ts`'s build-version `define` — is
// deliberately NOT declared here: `shell/build-version.ts` is compiled by
// the node project too (it feeds `build-version.node.ts`), which does not
// include this file, so the declaration lives in that module instead.

interface ImportMetaEnv {
  /** The Google OAuth client id issue #73 introduced. #583/#584 moved every
   * token request — silent re-mint, rotation, and the interactive
   * Connect/Reconnect press — onto the authority
   * (`calendar/authority-token-client.ts`, ADR-0028), which authenticates
   * with the device token and never reads this id. #585 answered the
   * question the previous revision of this comment left open: the Settings
   * calendar section now gates on `taskTokenState` (whether this device
   * holds a device token), not on this variable, so **nothing in
   * `client/web` reads it any more.** It is dead, not yet deleted — retiring
   * the browser OAuth client itself (Google console, this variable, and its
   * Actions/CSP plumbing) is #586's charter. */
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
