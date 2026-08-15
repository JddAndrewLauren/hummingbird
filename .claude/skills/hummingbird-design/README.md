# Hummingbird Design System

Hummingbird is a personal task tracker and unified dashboard — a GTD-style
system for one person, not a team tool. It captures what's on your mind,
sorts it into a funnel, computes what to do right now, and lets a small set
of standing rules interrupt you when the world demands it.

Planned surfaces: **desktop web**, **native Android**, **Android Wear**, and
**iOS**. All four are views onto one local sync engine — the engine owns the
mirror, the outbound queue and the sync cycle; a view renders published state
and asks the engine for things. Closing a window never loses unfinished work.

## Sources this system was built from

| Source | What it gave us |
| --- | --- |
| https://github.com/JddAndrewLauren/hummingbird (branch `main`, commit `d4105b5`) | Domain vocabulary (`CONTEXT.md`), architecture decisions (`docs/adr/0001`–`0012`), the only shipped UI (`client/web/src/App.tsx`, `calendar/ContextTile.tsx`, `calendar/CalendarPicker.tsx`), the app's `theme-color` `#0b0f14` |
| `uploads/light-1024.png`, `uploads/dark-1024.png` | The app icon, light and dark plates — the source of the whole colour palette |
| `uploads/concept-sheet.png` | The icon design rationale and its size ladder (1024 → 16px), copied to `assets/icon-concept-sheet.png` |

**Explore the repository yourself** — <https://github.com/JddAndrewLauren/hummingbird>
— before designing anything substantial. `CONTEXT.md` is a glossary of the
product's entire conceptual model and is the single best guide to what words
belong on screen; `docs/adr/` explains why the product behaves the way it
does. Neither is reproduced in full here.

### What did *not* exist in the sources

The web client is deliberately a placeholder shell (ADR-0006, issue #69):
Tailwind's default slate palette, system font stack, no design language, no
component library, no icon set, no typeface. There is **no logo file** — the
app icon PNGs are the only brand mark, and the `client/web/public/icon.svg`
in the repo is an unrelated cyan placeholder, not the brand. Nothing here
reconstructs a mark that doesn't exist: where a logotype is needed, the name
is set in type (see the Wordmark card).

So: colours, type, spacing, motion and every component below were **authored
for this system**, anchored to the icon's palette and the product's own
vocabulary. The one faithful recreation of shipped UI lives in
`ui_kits/web/SettingsScreen.jsx` (`ShippedShell`).

---

## CONTENT FUNDAMENTALS

The product's own writing — in `CONTEXT.md`, the ADRs and the shipped strings
— is unusually consistent, and the UI inherits it.

**Voice.** Plain, declarative, slightly dry. It explains mechanisms rather
than selling them. Sentences state what is true and stop: *"No current or
upcoming event."* *"Unavailable — uncheck to stop polling it."* *"1 edit
didn't apply."*

**Person.** Mostly impersonal — the UI describes the system's state, not the
user's feelings. "You" appears only when the user must act. Never "we". Never
"let's".

**Casing.** The product name is **lowercase everywhere**: `hummingbird`, in
the title bar, the nav rail and the app header. Sentence case for everything
else — labels, buttons, headings. UPPERCASE only in the 11px mono meta style
(`AS OF 12M AGO`, `SIZE:DEEP`, `ION-142`).

**Domain words are exact.** These are terms of art; do not paraphrase them.
Item, Stage, Action, Route, Destination, Fog, Mint, Step, External wait,
Capture source, Context source, Mirror, Urgency, Deadline, Scheduled date,
Size, Energy, Context, Alert, Rule, Promotion, Tier, Delivery log, Ack,
Context snapshot, Standing
question, Cycle, Outbound queue, Dead-letter journal, View. Say "mint an
action", not "create a task". Say "ack", not "dismiss" — they mean different
things. "Blocked" means an external wait and nothing else.

**Honesty over reassurance.** The product's strongest habit: when data is old
it says so and keeps showing it (*"Stale — as of 42m ago"*), rather than
hiding it or pretending. Empty states are reported as facts, and an empty
inbox is good news, not an apology.

**Numbers and time.** Relative and short: `just now`, `12m ago`, `3h ago`,
`Fri`, `Mon`. Counts are bare (`4 unsorted`, `3/7`).

**Punctuation.** Em dash for the honest aside (`Stale — as of…`), middle dot
for metadata joins (`Fly · hb-worker · 6m`). No exclamation marks. No
questions asked of the user unless the UI can act on the answer.

**Emoji: never.** Not in UI copy, not in labels, not in empty states. The one
piece of warmth in the brand is the bird itself and the way things move.

Examples of the register, written for this system:

> Everything captured has been sorted. The sweeper drains again in 15 minutes.

> Steps are 2–5 minute physical actions. They live on an action's checklist
> and never become actions themselves.

> Default-deny: what no rule matches stays silent.

---

## VISUAL FOUNDATIONS

**Where the palette comes from.** Every colour is sampled from the app icon.
*Ember* is the bird's throat (`#eb6d06` primary, `#c62704` deep,
`#fe8a03` warm); *Ink* is the dark plate (`#2f3a45`) extended down to the
client's own `theme-color` (`#0f141a`); *Sand* is the light plate
(`#faf0e7`); *Plume* greys are the head feathers. Status colours — moss,
amber, crimson, sky — were added because the domain needs six stages and four
urgency levels, and orange alone cannot carry them.

**Colour discipline.** One accent, used sparingly: the primary button, the
active nav item, the "in progress" stage, the FAB. Ember is never a
background wash and never a gradient. Status colour means *meaning* — a
coloured pill always encodes stage, tier or urgency, never decoration. Light
mode is warm paper (`--sand-50`) with white cards; dark mode is ink 950 with
ink 800 cards. Themes switch on `[data-theme="dark"]`.

**Type.** Space Grotesk for display and headings (bold, tracking −0.022em at
display size), Figtree for body at 15px/1.55, Space Mono at 11px uppercase
+0.08em for machine values. Three families, eight sizes, nothing between
them. The mono meta style is the system's signature: it makes timestamps,
ids and stage names read as *data the system computed*, distinct from words a
human wrote.

**Spacing and layout.** A 2/4/6/8/12/16/20/24/32/40/48/64 scale. Fixed
layout constants: a 236px nav rail, a 320px right-hand context panel, an
880px content column, and a 44px row height that doubles as the minimum
touch target on every surface. The rail and the panel are fixed; only the
centre column scrolls.

**Backgrounds.** Flat colour. No photography, no illustration, no repeating
pattern, no texture, no gradient meshes. The single image in the whole system
is the app icon. This is deliberate: the product is a dense read-at-a-glance
dashboard, and imagery would compete with status colour. Where a surface
needs to recede, it uses `--surface-quiet` (a 4% tint), not a picture.

**Cards.** 14px radius, 1px hairline border (`--border-subtle`), and a soft
shadow — `--shadow-1` at rest. Never a coloured left border. The one card that
is the answer on screen gets `accent` (an ember-tinted border), not a fill.
Cards nest at most two elevations deep in a region.

**Corner radii.** 4 / 6 / 10 (controls) / 14 (cards) / 20 / 28 (sheets) /
pill. The app icon's own plate is a 22.37% squircle — use
`--radius-icon-app` whenever the icon is rendered.

**Shadows.** Four steps plus one accent glow. Light-mode shadows are
ink-tinted and low-contrast (6–14% alpha); dark-mode shadows are near-black
and deeper. There is no inner-shadow system beyond a single hairline
highlight (`--shadow-inset-hair`) available for pressed surfaces. Focus is a
3px ember ring at 38% (`--ring-focus`), never a colour change alone.

**Motion.** A hummingbird beats fast and stops dead — motion is 90–320ms,
always interruptible, and nothing drifts or floats. `--ease-flit`
(`cubic-bezier(.2,.8,.2,1)`) is the default. `--ease-hover`
(`cubic-bezier(.34,1.4,.64,1)`) carries the whimsy: a small overshoot on
hover, which is the only place the system is allowed to be playful. Page
transitions use `--ease-out-soft`. All of it collapses to 1ms under
`prefers-reduced-motion`.

**Hover states.** Buttons darken one accent step and lift 1px; ghost buttons
gain a `--surface-quiet` background; secondary buttons deepen their border and
step up one shadow; cards marked `interactive` lift 1px and go to
`--shadow-2`; rows tint to `--surface-quiet`. Never opacity fades — a hovered
thing gets *more* solid, not less.

**Press states.** `scale(.97)` and the shadow drops to none, so the control
sits down onto the page. Primary buttons also darken to `--accent-press`.

**Borders.** 1px, always. Three weights of meaning: `--border-subtle` (card
edges), `--border-default` (controls), `--border-strong` (hover, dividers
that must be seen). In dark mode borders are white at 7 / 12 / 22%.

**Transparency and blur.** Used in exactly two places: the scrim behind a
bottom sheet (`--surface-scrim`), and the iOS tab bar capsule (82% card
colour + 18px backdrop blur). Nowhere else — a translucent dashboard is a
hard-to-read dashboard.

**Imagery.** There is none, beyond the icon. The icon's own treatment is
warm, low-poly, high-contrast, and it is never recoloured, cropped, rotated,
or placed on a coloured plate of its own — the plate is part of the artwork.

**Protection gradients vs capsules.** Capsules, always: floating chrome sits
on a bordered, shadowed surface rather than a fade-to-transparent gradient.

---

## ICONOGRAPHY

**The sources contain no icon set.** The repository ships one SVG
(`client/web/public/icon.svg`), a cyan placeholder unrelated to the brand,
and no glyph library; the shipped shell uses plain text buttons.

**Substitution (please confirm):** this system uses **Lucide**
(`https://unpkg.com/lucide@0.469.0/dist/umd/lucide.js`) from CDN — outline
icons, 1.75px stroke, rounded caps, which sits well with Space Grotesk's
geometry and the icon's clean planes. Every card and UI kit loads it, and the
`Icon` component wraps it so no SVG is ever hand-inlined. If Hummingbird
picks a different set, `Icon.jsx` is the only file that changes.

- **Sizes:** 13px inside pills, 16px inline in text, 17–18px in buttons,
  20px in toolbars and on touch, 24px+ only in empty states.
- **Colour:** `currentColor`. Icons never carry colour independently of their
  label.
- **Named vocabulary in use:** `feather` (capture — the brand's own verb),
  `inbox` (triage), `route`, `zap` (now / next up), `bell` / `siren` /
  `bell-off` (the notification lane), `calendar-clock` and `radio` (context
  tile: upcoming vs. in progress), `cloud-fog` (fog), `list-checks` (steps),
  `flag` (deadline), `calendar` (scheduled date), `link` (blocked by),
  `refresh-cw`, `check`, `sparkles` (mint).
- **Emoji:** never. **Unicode as icons:** never — the middle dot `·` in meta
  lines is punctuation, not an icon.
- **Raster icons:** only the app icon PNGs in `assets/`.

---

## Components

Sixteen components, grouped by concern. Each has a `.jsx`, a `.d.ts` props
contract, a `.prompt.md` with usage, and one preview card per directory.

**`components/core/`** — `Icon`, `Button`, `IconButton`, `Card`, `Badge`

**`components/forms/`** — `Input`, `Select`, `Slider`, `Checkbox`, `Switch`

**`components/domain/`** — `StageBadge`, `ItemRow`, `ContextTile`,
`AlertCard`, `CalendarPicker`

**`components/feedback/`** — `EmptyState`

### Intentional additions

The sources define no component library, so this inventory was authored. Two
notes on how it was chosen:

- `ContextTile` and `CalendarPicker` are **recreations** of real components
  (`client/web/src/calendar/ContextTile.tsx` and `CalendarPicker.tsx`) —
  same props, same states, same staleness and unavailable-calendar
  behaviour, restyled onto these tokens.
- `Icon` is a wrapper over Lucide rather than a brand component; it exists so
  that no screen ever hand-inlines an SVG.
- `StageBadge`, `ItemRow` and `AlertCard` are domain-shaped rather than
  generic: they encode ADR-0009's stage vocabulary and ADR-0012's alert
  lane. Prefer them over a generic list row.
- `Slider` exists for the capture form's optional metadata — **energy**
  (`low / medium / high`), **size** (`quick / normal / deep`) — alongside a
  `Select` for **context** (`@home`, `@computer`, `@phone`, `@errands`,
  `@garden`, `@waiting`). All three are optional on every surface: unset is
  the default and a legitimate resting state, because deciding is mint-time
  work, not capture-time work.

---

## UI kits

| Kit | Directory | Screens |
| --- | --- | --- |
| Desktop web | `ui_kits/web/` | Now, Triage, Routes, Alerts, Settings (+ a recreation of the shipped shell) |
| iOS | `ui_kits/ios/` | Now, Capture, Action detail |
| Android | `ui_kits/android/` | Now, Alerts, Capture sheet |
| Wear OS | `ui_kits/wear/` | Next up, Urgent alert, Calendar context |

Only `ShippedShell` in the web kit recreates existing UI. Everything else is
proposed and built from the domain vocabulary — treat the kits as the visual
target for the four planned surfaces, not as a record of what is built.

---

## Index

```
styles.css              the one file consumers link (imports only)
tokens/                 fonts · colors · typography · spacing · radius · elevation · motion · base
components/             core · forms · domain · feedback  (jsx + d.ts + prompt.md + card html)
ui_kits/                web · ios · android · wear  (each with its own README.md)
guidelines/             20 specimen cards: Brand · Colors · Type · Spacing · Motion
assets/                 app-icon-light-1024.png · app-icon-dark-1024.png · icon-concept-sheet.png
thumbnail.html          the system's homepage tile
github.md               source repository association and sync record
SKILL.md                Agent Skills entry point
readme.md               this file
```

## Open questions for the team

1. **Typefaces.** No brand typeface exists in the sources. Space Grotesk +
   Figtree + Space Mono is a proposal, loaded from Google Fonts. If
   Hummingbird has licensed fonts, drop the files in `assets/fonts/` and
   rewrite `tokens/fonts.css`.
2. **Icons.** Lucide is a substitution (see ICONOGRAPHY).
3. **Logo.** None exists; the wordmark is plain type. If a mark is drawn
   later, add it to `assets/` and update the Wordmark card.
