import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import tailwindcss from "@tailwindcss/vite";
import { VitePWA } from "vite-plugin-pwa";

// client/web (#69): Vite + React + Tailwind + TS shell. The wasm core
// (hummingbird-ffi-web, #67) loads in a Web Worker via vite-plugin-wasm +
// vite-plugin-top-level-await (ADR-0003/ADR-0006). vite-plugin-pwa ships the
// offline app-shell service worker; deploy config lives in wrangler.toml.
export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    wasm(),
    topLevelAwait(),
    VitePWA({
      registerType: "autoUpdate",
      // Defaults to true, which force-adds every manifest icon to the
      // precache regardless of the workbox globs below — that is what kept
      // dragging the 238 KB install icon in. Off, the globs decide, and the
      // 192 still gets precached because it matches them.
      includeManifestIcons: false,
      workbox: {
        // The wasm binary is fetched by the worker and must be precached
        // for the "installed PWA loads with the network disabled"
        // acceptance criterion. `png` is in here because the nav rail's
        // brand mark is raster: leave it out and the shell renders a broken
        // image offline.
        globPatterns: ["**/*.{js,css,html,svg,png,wasm}"],
        // ...except the 512 install icon, which the platform fetches once at
        // install time and the running shell never renders. Precaching it
        // would put 238 KB on every first load to no end. No `**/` prefix:
        // these patterns resolve against the build root, and `**/` there
        // requires a directory to descend into, so it silently misses a
        // top-level file (verified against the emitted sw.js, not assumed).
        globIgnores: ["app-icon-512.png"],
        // Workbox refuses to precache a file over 2 MiB and, in
        // vite-plugin-pwa, that refusal is a *build error*, not a warning.
        // The wasm core crossed it by under 4 KB (2,101,037 bytes against
        // 2,097,152) when ADR-0015 grew `hummingbird-domain`, which the
        // client compiles too — so a server-side change turned the client
        // build red, and `deploy-client.yml`'s deploy job (`needs: test`)
        // stopped being reachable at all.
        //
        // Raised rather than worked around: the glob above deliberately
        // includes `wasm` because the "installed PWA loads with the network
        // disabled" criterion needs the core precached, so excluding it to
        // get a green build would trade a broken build for a broken
        // offline app — the failure the acceptance criterion exists to
        // catch, moved somewhere it is not checked.
        //
        // 4 MiB, not 2.1: the binary has been creeping toward the default
        // for a while and arrived under 4 KB clear of it. A limit set just
        // above today's size would re-break on the next domain type added,
        // in a client build, for a reason written in the server crate.
        // There is still a ceiling, so genuine bloat fails loudly.
        maximumFileSizeToCacheInBytes: 4 * 1024 * 1024,
      },
      manifest: {
        name: "hummingbird",
        short_name: "hummingbird",
        description: "Personal GTD-style task system.",
        start_url: "/",
        scope: "/",
        display: "standalone",
        // --ink-950, the dark theme's page surface. Matches index.html's
        // theme-color, which these two had drifted from.
        background_color: "#0f141a",
        theme_color: "#0f141a",
        icons: [
          {
            src: "app-icon-192.png",
            sizes: "192x192",
            type: "image/png",
            purpose: "any",
          },
          {
            src: "app-icon-512.png",
            sizes: "512x512",
            type: "image/png",
            // `any` only — deliberately not `maskable`, for two reasons.
            // A maskable icon must be full-bleed, and these plates have
            // transparent corners (the reference artwork is a squircle, and
            // the derivation lifts the surround out rather than keeping it
            // white); a platform mask wider than that squircle would expose
            // the gaps. It must also keep its content inside a circle of 80%
            // the icon's diameter, and this artwork does not: measuring the
            // bird against the plate's own background, 33.6% of its pixels
            // fall outside that circle and containment would need a 0.57
            // scale. Both want a separately produced maskable plate, not a
            // flag here — claiming `maskable` without one is how you ship a
            // cropped icon. `any` makes the platform pad it instead. (The
            // placeholder this replaced claimed `maskable` while also having
            // transparent corners, so it had the same bug.)
            purpose: "any",
          },
        ],
      },
    }),
  ],
  // The dev half of ADR-0006/0008's same-origin API. In production a
  // path-scoped Worker route puts `hb.twinion.net/api/*` on the authority
  // worker in front of the shell's Custom Domain (see
  // `server/worker/wrangler.toml`); here the dev server plays that role, so
  // `core.worker.ts` can supply `self.location.origin` as the authority's
  // base URL in both places and nothing has to be configured per
  // environment. Cross-origin would not work anyway: the authority sends no
  // `Access-Control-*` header at all, by decision (ADR-0008, "No CORS
  // anywhere").
  //
  // 8787 is `wrangler dev`'s default port and `smoke.sh`'s `SMOKE_PORT`
  // default. With no `wrangler dev` running, `/api` calls fail connection —
  // which is the honest offline state, and the shell is built for it.
  server: {
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8787",
        changeOrigin: false,
      },
    },
  },
  worker: {
    format: "es",
    plugins: () => [wasm(), topLevelAwait()],
  },
});
