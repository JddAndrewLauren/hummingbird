One item in a list: urgency dot, title, then right-aligned metadata and the stage badge.

```jsx
<ItemRow title="Order the replacement sensor" stage="ready" urgency="soon"
  due="Fri" size="quick" steps="2/5" />
```

Keep metadata right-aligned and monospaced; the title is the only thing in sans. Due dates take colour from urgency, scheduled dates never do.
