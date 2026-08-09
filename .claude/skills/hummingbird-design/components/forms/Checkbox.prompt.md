Checkbox with label and optional hint; also the microtask Step tick.

```jsx
<Checkbox checked={done} onChange={toggle} label="Email the shop about the part" />
<Checkbox checked tone="warn" label="team_calendar@…" hint="Unavailable — uncheck to stop polling it" />
```

The real `<input>` is visually hidden behind the 18px box, so the control stays in
the tab order and announces normally. Keyboard focus paints `--ring-focus` on the
box; a pointer click does not, because the ring follows the input's
`:focus-visible` state.

Anything else you pass lands on the root `<label>` — `id`, `className`, `data-*`,
`aria-describedby`, `onMouseEnter` — so a caller can wire it into a form or
describe it further. Internal hover handlers still run alongside yours.
