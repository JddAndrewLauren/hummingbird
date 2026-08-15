One item in a list: urgency dot, title, then right-aligned metadata and the stage badge.

```jsx
<ItemRow title="Order the replacement sensor" stage="ready" urgency="soon"
  deadline="Fri" size="quick" steps="2/5" onClick={open} />
```

Keep metadata right-aligned and monospaced; the title is the only thing in sans. Deadlines take colour from urgency, scheduled dates never do.

`onClick` is what makes a row a button. With it the row gets `role="button"`, a tab stop, a pointer cursor, and Enter/Space activation — a div gets none of that for free. Without it the row is inert text: no role, no tab stop, no pointer. Do not fake a click target by styling one.

The urgency dot's tooltip reads in words ("Deadline soon"), not as the stored enum. `onMouseEnter`, `onMouseLeave` and `onKeyDown` passed in are called after the row's own, not instead of them.
