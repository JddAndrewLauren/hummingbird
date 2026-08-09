// Demo mode renders the design system's kit fixtures instead of the app's
// (currently empty) real data, so the shell can be checked against the kit
// screens before any task data exists. It is gated twice — see demo.ts.

export function isDemoEnabled(search: string): boolean {
  return new URLSearchParams(search).has("demo");
}
