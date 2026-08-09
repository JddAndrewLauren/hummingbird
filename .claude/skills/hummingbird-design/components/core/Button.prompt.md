The standard action control — one `primary` per view, everything else `secondary`/`ghost`.

```jsx
<Button iconLeft="plus">Capture</Button>
<Button variant="secondary" size="sm">Refresh calendar</Button>
<Button variant="ghost" iconLeft="check">Ack</Button>
```

Variants: primary, secondary, ghost, quiet (brand tint), danger. Sizes sm 30 / md 36 / lg 44 px — use `lg` on touch surfaces. Hover lifts 1px with a slight overshoot ease; press scales to .97.
