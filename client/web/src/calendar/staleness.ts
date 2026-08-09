// How the context tile (issue #73) decides it's showing honestly-stale
// data — the Agent Brief's "degrades to stale honestly" criterion. The
// foreground timer polls every 15 minutes (#46, under ADR-0005); a margin beyond that
// is what separates "the last poll simply hasn't ticked yet" from "polling
// is actually stuck" (offline, held on a credential, backgrounded tab).

export const STALE_AFTER_MS = 20 * 60 * 1000; // 15-minute cadence + 5-minute slack

export function isStale(asOfMs: number, nowMs: number): boolean {
  return nowMs - asOfMs > STALE_AFTER_MS;
}

/** A short, human "as of" label: `"just now"`, `"12m ago"`, or `"3h ago"`. */
export function formatAsOf(asOfMs: number, nowMs: number): string {
  const deltaMs = Math.max(0, nowMs - asOfMs);
  const minutes = Math.floor(deltaMs / 60_000);
  if (minutes < 1) {
    return "just now";
  }
  if (minutes < 60) {
    return `${minutes}m ago`;
  }
  const hours = Math.floor(minutes / 60);
  return `${hours}h ago`;
}
