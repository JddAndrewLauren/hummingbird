# UI kit — desktop web

Five click-through screens of the Hummingbird desktop web client, composed
entirely from this system's components.

| Screen | File | What it shows |
| --- | --- | --- |
| Now | `NowScreen.jsx` | Top pick, also-startable list, calendar context tiles, standing questions, context snapshots |
| Triage | `TriageScreen.jsx` | Capture card (field + optional energy/size sliders + context dropdown) and the unsorted queue with mint / grill / drop |
| Routes | `RouteScreen.jsx` | Destination, minted actions, fog, and the microtask step checklist |
| Alerts | `AlertsScreen.jsx` | The notification lane: tiers, ack, and the rules that emit deliveries |
| Settings | `SettingsScreen.jsx` | Calendar picker, device preferences, mirror — plus a recreation of the shipped shell |

## What is real and what is proposed

`SettingsScreen.jsx`'s right-hand column (`ShippedShell`) is a faithful
recreation of the only Hummingbird UI that exists in the repository today:
`client/web/src/App.tsx` at `d4105b5` — a deliberate placeholder shell on
Tailwind's slate defaults (ADR-0006 / issue #69), carrying the calendar
opt-in, the context tile and the picker.

Everything else is **proposed**: it is built from the domain vocabulary in
`CONTEXT.md` and the ADRs, not copied from a shipped screen, because no
shipped screen exists. Treat these as the visual target, not a record.

Run it by opening `index.html`; the theme toggle sits at the bottom of the rail.
