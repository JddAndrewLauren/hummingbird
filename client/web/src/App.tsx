import { useStore } from "./store/useStore";

// Placeholder shell (#69): proves the worker + wasm-core + offline load
// path, nothing more. Real UI (Linear sync, auth, calendar tile) is
// explicitly out of scope here — see #73 for auth/tile.
export function App() {
  const status = useStore((state) => state.status);
  const apiVersion = useStore((state) => state.apiVersion);
  const error = useStore((state) => state.error);

  return (
    <main className="flex min-h-screen flex-col items-center justify-center gap-4 bg-slate-950 p-8 text-slate-100">
      <h1 className="text-2xl font-semibold">hummingbird</h1>
      {status === "loading" && <p data-testid="status">Loading core…</p>}
      {status === "ready" && (
        <p data-testid="status">
          Core ready (api v{apiVersion}) — worker + wasm loaded.
        </p>
      )}
      {status === "error" && (
        <p data-testid="status" className="text-red-400">
          Core failed to load: {error}
        </p>
      )}
    </main>
  );
}
