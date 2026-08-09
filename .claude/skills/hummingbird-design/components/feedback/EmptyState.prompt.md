Says what an empty region means, in plain words, and offers the one thing to do about it.

```jsx
<EmptyState icon="inbox" title="Triage is empty" headingLevel={3}
  body="Everything captured has been sorted. The sweeper drains again in 15 minutes."
  action={<Button variant="secondary" iconLeft="plus">Capture something</Button>} />
```

An empty inbox is good news — write it that way. Never use an empty state to apologise.

The title is a real heading, so the empty region is something assistive tech can navigate to. `--type-h3` is the size token, not the level: set `headingLevel` from where the state sits — `2` when it is a whole column's content under the page's `h1`, `3` inside a section that already has an `h2`. Levels must not skip. Default is `2`.
