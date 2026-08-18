import type { KindRegistryDTO } from "../../store/protocol";

// The rules editor's cascade — kind, then field, then operator, then value
// widget. Everything reads the exported kind registry (#133,
// `hummingbird_domain::kind_registry_json`); nothing here is a second,
// hand-maintained copy of what kinds or fields exist.
//
// The *field* half of that cascade — `fieldsForKind` and `fieldType` — no
// longer lives here: it is `hummingbird_core::decisions::rules::validity`
// (ADR-0025, #141/M4, #540), because the phone's create-and-edit form
// needs the identical narrowing (ADR-0013's "any kind" means the Event
// core alone; a named kind means core-first, a colliding kind field
// skipped) and a Kotlin copy would have been the third.
//
// The *kind* half below stays TS: `kindLabel` is display copy, and
// `kindOptions` is that copy plus the registry's own declaration order —
// two clients wording "Calendar event" differently is a difference, not a
// bug (ADR-0025's own test for what belongs in core).

export { fieldsForKind, fieldType } from "../../decisions/seam";

/** One selectable kind option — `null` is ADR-0013's "any kind" (a `NULL
 * event_kind`), always first, since it is the widest and most common
 * choice ("any alert-worthy stream"). */
export interface KindOption {
  key: string | null;
  label: string;
}

/** Every selectable kind, "any kind" first, then every registry entry in
 * declared order — the registry's own order, never re-sorted, since
 * `EVENT_KINDS`'s declaration order is meaningful launch order upstream. */
export function kindOptions(registry: KindRegistryDTO): KindOption[] {
  return [
    { key: null, label: "Any kind" },
    ...registry.kinds.map((kind) => ({ key: kind.key, label: kindLabel(kind.key) })),
  ];
}

/** Human copy for a kind key. Falls back to the raw key for one this build
 * does not have a human label for yet — the same "never hide an unknown
 * key" reading `bindings.ts`'s `bindingCopy` gives an unrecognised
 * binding, since a kind can be added to the registry with no UI-side
 * change and must still render as *something* legible. */
export function kindLabel(key: string): string {
  const known: Record<string, string> = {
    email: "Email",
    calendar_event: "Calendar event",
    item_threshold: "Item",
    snapshot_change: "Snapshot change",
    alert_raised: "Alert raised",
  };
  return known[key] ?? key;
}
