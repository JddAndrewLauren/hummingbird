# Context — personal task system (hummingbird)

Glossary for the personal GTD-style task system on Linear (org `twinion`, team `ION`). The wayfinder map is [issue #1](https://github.com/JddAndrewLauren/hummingbird/issues/1). This file is a glossary only — no implementation details.

## Terms

- **Action** — the unit of personal project work: a concrete outcome that is *startable without further planning* and *finishable within one `size` label* (`deep` is the ceiling). Bigger than `deep` → it's a sub-route, not an action; not startable without more decisions → it's **fog**, not an action yet. Minted as a Linear issue by `/to-actions`. (Deliberately not "slice" — that word belongs to the twinion code pipeline, where it means a vertical, demoable cut.)
- **Route** — the plan section a project carries in its Linear project description: Destination, Fog, Notes, and the ordered list of minted actions. Owned and refreshed by `/to-actions`. Holds only what issues can't.
- **Destination** — what "done" looks like for a project, in the human's own terms.
- **Fog** — a segment of a route that can't yet be defined as an action, recorded with the open question that blocks defining it. Fog stays in the Route; everything definable gets minted.
- **Mint** — create the Linear issue for a defined segment (proposed labels, human-confirmed, landing in Ready). Every definable segment is minted immediately, startable or not; sequencing is carried by native `blocked by` relations, so closing an action automatically frees its dependents.
- **Step** — a microtask: one ~2–5-minute concrete physical action inside a checklist in an issue's body. Below tracker granularity — never a sub-issue. Written by `/microtask`.
- **External wait** — the only meaning of the Blocked state: the world is making the work wait (a callback, a part in the mail). Never used for inter-action dependencies — those are `blocked by` relations between minted actions.
- **Capture source** — a source holding unprocessed thoughts, which the system *drains*: each item is created in Linear Triage first, then acked in the source. An inbox the system empties. (Google Tasks is the first one.)
- **Context source** — a source that is the authority for its own domain, consumed read-only at decision time and **never drained** — its records are never materialized as Linear issues. (Calendars are the archetype.)
- **Mirror** — a device's derived, disposable, unified local read model of all the authorities (tasks from Linear, events from context sources). Never a source of truth: deleting it loses nothing, and no record originates truth there. Named in ADR-0001; extended here to hold context-source replicas.
- **Urgency** — a derived, time-varying property of items, computed by consumers at read time over the mirror. Never a stored class and never a routing decision at ingestion: the same item may be background context on Monday and demand action ten minutes before its moment.
