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
