/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** The Google OAuth client id GIS consent (issue #73) requests
   * `calendar.readonly` against — the sweeper's Workspace Internal OAuth
   * client id may be reused here (an independent per-device credential;
   * its refresh token and Tasks/Gmail scopes are untouched). Unset in dev
   * by default: the calendar opt-in UI simply doesn't render without it. */
  readonly VITE_GOOGLE_CLIENT_ID?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
