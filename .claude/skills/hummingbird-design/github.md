repo: JddAndrewLauren/hummingbird
branch: main
path: (whole repo — `CONTEXT.md`, `docs/adr/`, `client/web/`)

## Last sync

date: 2026-08-09T20:05:16Z

### Updated in this project

- Built the token system from the app icon palette and the client's `theme-color`.
- Recreated `ContextTile` and `CalendarPicker` from `client/web/src/calendar/`.
- Recreated the shipped placeholder shell (`client/web/src/App.tsx`) inside the web UI kit.
- Wrote content and voice guidance from `CONTEXT.md` and `docs/adr/0009`/`0012`.

## Screen map

| Screen / file | Built from |
| --- | --- |
| `ui_kits/web/SettingsScreen.jsx` (`ShippedShell`) | `client/web/src/App.tsx` |
| `components/domain/ContextTile.jsx` | `client/web/src/calendar/ContextTile.tsx`, `calendar/staleness.ts` |
| `components/domain/CalendarPicker.jsx` | `client/web/src/calendar/CalendarPicker.tsx`, `calendar/selection.ts` |
| `components/domain/StageBadge.jsx`, `ItemRow.jsx` | `CONTEXT.md`, `docs/adr/0009-the-owned-schema-and-context-lanes.md` |
| `components/domain/AlertCard.jsx`, `ui_kits/*/Alerts*` | `docs/adr/0012-the-notification-lane.md` |
| `ui_kits/web/*`, `ui_kits/ios/*`, `ui_kits/android/*`, `ui_kits/wear/*` | `CONTEXT.md` glossary (proposed screens — no shipped counterpart) |
| `tokens/colors.css` | `uploads/*-1024.png` app icons, `client/web/index.html` `theme-color` |

## Last push (mirror -> design project)

date: 2026-08-13
direction: this mirror was the source; 2 files written to the design project

Terminology corrections made in the repo (docs-only slices) and pushed up so
the mirror and the design project do not fork — a re-pull would otherwise have
reverted them and reintroduced the retired term "Due date":

- `README.md` -> `readme.md` (from #400, commit `107c605`) — three changes in
  one file: the "domain words are exact" roster drops "Due date" for
  **Deadline** and gains **Size**, **Energy** and **Context** (the three axes
  ADR-0021 turned into on-screen controls); the `flag` icon gloss reads
  "deadline"; and `Slider`'s size scale is corrected to
  `quick / short / deep` per the schema (it read `quick / normal / deep`).
  The size-scale fix is not terminology but rides along in the same file.
- `components/forms/Input.prompt.md` (from #408, in batch PR #433) — the
  example field is `label="Deadline"` with error copy "Leave blank if nothing
  breaks."; the old "Due dates are deadlines" phrasing was redundant once the
  label was renamed.

Both remote files were byte-identical to their pre-edit repo versions before
the write, so nothing authored in the design project was overwritten.

Still forked, and *not* part of this push:
`components/domain/ItemRow.prompt.md` still says "Due dates take colour from
urgency". That line predates the 2026-08-09 push, so the mirror and the design
project agree on it — it needs an in-repo fix before there is anything to push.

### Previous push

date: 2026-08-09
direction: this mirror was the source; 23 files written to the design project

Accessibility and type-contract fixes found reviewing the app's shell rebuild
(PR #148). They were made here and in `client/web/src/components/` together,
then pushed, so the mirror and the design project do not fork:

- `Checkbox`, `Switch` — a visible `--ring-focus` ring on the proxy (the real
  input is visually hidden, so there was no focus indicator at all: WCAG
  2.4.7); props opened to `LabelHTMLAttributes` so callers can pass `id`,
  `aria-*` and `data-*`; `Switch`'s hardcoded `#fff` thumb tokenised to
  `--on-accent` (it was the only raw colour in all 16 components and was wrong
  in dark mode); `readOnly` when no `onChange` is supplied.
- `ItemRow` — Enter/Space activation (`role="button"` with no `onKeyDown` was
  unreachable by keyboard: WCAG 2.1.1); the role, tab stop and pointer are now
  conditional on `onClick`, so a row that does nothing is no longer a phantom
  button; the tooltip shows a word, not the raw urgency enum.
- `Input` — `aria-describedby` to the hint/error node and `aria-invalid` on
  error.
- `Slider` — `role="slider"` made conformant: arrows `preventDefault`, full
  Arrow/Home/End key set, and `aria-valuenow` always present, using a `-1`
  sentinel (with `aria-valuemin={-1}`) so "not set" stays distinguishable from
  a deliberate pick of the lowest option.
- `EmptyState` — renders a real heading with a `headingLevel` prop, instead of
  a `<p>` styled to look like one.
- `Card` — `as` narrowed to a container-element union; the previous
  `keyof JSX.IntrinsicElements` admitted void elements (`as="input"`
  type-checked and threw at runtime).
- `CalendarPicker.d.ts` — `Omit` the DOM's own `onToggle`, which collided with
  this component's id-taking one.
- `Button`, `IconButton`, `Card`, `ItemRow`, `Input` — `{...rest}` spread
  before the internal pointer/focus handlers, which now compose with the
  caller's rather than being silently replaced by them.

## Local pull (this copy)

date: 2026-08-09
designProjectId: dcdceb10-7954-425a-903c-aa8b13bbb158 ("Hummingbird Design System" on claude.ai/design)
account: the design project is connected to the WORK Claude account (as of
2026-08-09) — DesignSync pulls/pushes need a session authorized on it
pulled into: `.claude/skills/hummingbird-design/` — the repo-local mirror of the design project

Pulled: SKILL.md, readme.md (as README.md), github.md, styles.css, `tokens/`,
all 16 components (`components/**` jsx + d.ts + prompt.md), `ui_kits/web/`.

Omitted (fetch on demand with `DesignSync get_file` against the projectId
above, or view on claude.ai/design):
- `guidelines/*.card.html` and `components/*.card.html` — Design System pane
  preview cards; the values they render live in `tokens/` and README.md
- `ui_kits/ios/`, `ui_kits/android/`, `ui_kits/wear/` — pull when those
  surfaces start
- `assets/*.png` (app icons, concept sheet) — binary; download from the
  design project when needed
- pane infra: `_ds_bundle.js`, `_ds_manifest.json`, `thumbnail.html`,
  `_adherence.oxlintrc.json`, `.thumbnail`, `uploads/`

App consumption: `tokens/` + `styles.css` are copied to
`client/web/src/design/` (see repo CLAUDE.md). When the design project
changes, re-pull here first, then re-copy into the app.
