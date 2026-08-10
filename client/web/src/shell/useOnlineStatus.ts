import { useEffect, useState } from "react";

// A thin wire-up over the browser's own connectivity signal — no decision
// logic to extract (`navigator.onLine` is already the whole answer), so
// unlike `sync-cadence.ts`/`sync-status.ts` this stays untested glue, the
// same convention `useCalendarWiring.ts`'s own `window`/`document` reads
// follow.
export function useOnlineStatus(): boolean {
  const [online, setOnline] = useState(() => navigator.onLine);

  useEffect(() => {
    function sync() {
      setOnline(navigator.onLine);
    }
    window.addEventListener("online", sync);
    window.addEventListener("offline", sync);
    return () => {
      window.removeEventListener("online", sync);
      window.removeEventListener("offline", sync);
    };
  }, []);

  return online;
}
