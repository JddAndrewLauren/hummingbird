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
      workbox: {
        // The wasm binary is fetched by the worker and must be precached
        // for the "installed PWA loads with the network disabled"
        // acceptance criterion.
        globPatterns: ["**/*.{js,css,html,svg,wasm}"],
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
            // The full-detail app icon. The tab favicon is a different file
            // (/favicon.svg, a reduced-shape variation that survives 16px);
            // this one is rendered large, so it wants every facet.
            src: "app-icon.svg",
            sizes: "any",
            type: "image/svg+xml",
            // `any` only — deliberately not `maskable`. A maskable icon must
            // keep its content inside a circle of 80% the icon's diameter,
            // because the platform may crop to any mask within that. This
            // artwork does not: measured against the outer silhouette on its
            // own 1024 canvas (safe radius 409.6), 9 of the 26 boundary
            // points fall outside, the beak tip (135,70) furthest in at
            // radius 580.9 — a circular mask would cut the beak off, and the
            // beak is the silhouette's whole identity. Containment needs
            // roughly a 0.60 scale, i.e. a separately generated maskable
            // variant, not a flag here. Claiming `maskable` without one is
            // how you ship a cropped icon; `any` makes the platform pad it
            // instead. (The previous placeholder claimed `maskable` *and*
            // had transparent rounded corners, which a mask would expose.)
            purpose: "any",
          },
        ],
      },
    }),
  ],
  worker: {
    format: "es",
    plugins: () => [wasm(), topLevelAwait()],
  },
});
