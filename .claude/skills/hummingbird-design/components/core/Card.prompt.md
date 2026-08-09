The surface every panel, tile and dialog sits on: 14px radius, hairline border, soft shadow.

```jsx
<Card elevation={1}><h3>Route</h3><p>Destination…</p></Card>
<Card interactive accent padding="var(--space-5)">Next up</Card>
<Card as="section" aria-label="Rules">…</Card>
```

Never stack more than two elevations in one region. `accent` marks the single most important card in a view.

`as` takes container elements only — `div`, `section`, `article`, `aside`, `nav`, `header`, `footer`, `main`, `form`, `fieldset`, `li`, `a`, `button`. Void elements are not offered: a Card always renders children, and `input` or `img` would throw. `onMouseEnter` and `onMouseLeave` passed in are called after the card's own hover tracking, not instead of it.
