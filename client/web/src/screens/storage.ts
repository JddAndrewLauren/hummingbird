// The narrow slice of `localStorage` the screens' device-local view
// preferences need.
//
// It used to live in `screens/triage-collapse.ts`, whose collapse preference
// died with Now's separate triage section — the captures are cards in the
// frontier's columns now, so there is no section to shut. The type outlived
// the module that happened to declare it first, which is what this file is:
// a home named after what it is rather than after the first preference that
// wanted it.
//
// Redeclared rather than imported from `lib.dom`'s `Storage` (the reasoning
// `shell/rail-collapse.ts` states): every consumer is injected a stub in
// tests, and a preference that cannot persist still applies for the session,
// so the three methods are all any of them may reach for.

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}
