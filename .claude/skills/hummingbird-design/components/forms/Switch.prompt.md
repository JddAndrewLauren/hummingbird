A settings toggle: label left, track right, for things that take effect immediately.

```jsx
<Switch checked={urgentOnly} onChange={toggle} label="Urgent tier only"
  hint="Normal-tier deliveries stay silent on this device." />
```

Use it for device/notification preferences; use Checkbox for list selection.

The real `<input role="switch">` is visually hidden behind the track, so the control
stays in the tab order and announces normally. Keyboard focus paints `--ring-focus`
on the track; a pointer click does not, because the ring follows the input's
`:focus-visible` state.

Anything else you pass lands on the root `<label>` — `id`, `className`, `data-*`,
`aria-describedby` — so a caller can wire it into a form or describe it further.
