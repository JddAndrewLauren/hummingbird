import type { KindEntryDTO, KindFieldDTO, KindRegistryDTO } from "../../store/protocol";

// The rules editor's cascade — kind, then field, then operator, then value
// widget — starts here. Everything reads the exported kind registry
// (#133, `hummingbird_domain::kind_registry_json`); nothing here is a
// second, hand-maintained copy of what kinds or fields exist. Adding a
// kind to the registry surfaces it with no change to this module.

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

function findKind(registry: KindRegistryDTO, key: string): KindEntryDTO | undefined {
  return registry.kinds.find((kind) => kind.key === key);
}

/** The field list a condition editor offers for `eventKind`. `null`
 * ("any kind") narrows the list to the Event core alone — ADR-0013's own
 * rule, and the acceptance criterion this function exists to satisfy.
 * A named kind offers the Event core plus that kind's own declared
 * fields, core fields first (core fields "always win a name collision"
 * per `hummingbird_domain::event`'s own doc, so listing them first mirrors
 * which one actually resolves at evaluation time) — a kind field sharing a
 * core name is skipped as a duplicate, never listed twice.
 */
export function fieldsForKind(registry: KindRegistryDTO, eventKind: string | null): KindFieldDTO[] {
  if (eventKind === null) {
    return registry.coreFields;
  }
  const kind = findKind(registry, eventKind);
  if (kind === undefined) {
    // A rule naming a kind this build's registry has never heard of —
    // ADR-0013's open registry key. Still offers the core alone, since
    // that is all this build can meaningfully edit; `validity.ts` is what
    // flags the rule itself as invalid, not this lookup.
    return registry.coreFields;
  }
  const coreNames = new Set(registry.coreFields.map((field) => field.name));
  return [...registry.coreFields, ...kind.fields.filter((field) => !coreNames.has(field.name))];
}

/** One field's declared type, within the field list `eventKind` offers —
 * `undefined` if `fieldName` is not in that list (the "invalid rule"
 * case `validity.ts` surfaces). */
export function fieldType(
  registry: KindRegistryDTO,
  eventKind: string | null,
  fieldName: string,
): KindFieldDTO["fieldType"] | undefined {
  return fieldsForKind(registry, eventKind).find((field) => field.name === fieldName)?.fieldType;
}
