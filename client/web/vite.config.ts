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
            src: "icon.svg",
            sizes: "any",
            type: "image/svg+xml",
            purpose: "any maskable",
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
