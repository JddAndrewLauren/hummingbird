The surface every panel, tile and dialog sits on: 14px radius, hairline border, soft shadow.

```jsx
<Card elevation={1}><h3>Route</h3><p>Destination…</p></Card>
<Card interactive accent padding="var(--space-5)">Next up</Card>
```

Never stack more than two elevations in one region. `accent` marks the single most important card in a view.
