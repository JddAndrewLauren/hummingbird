Read-only calendar context: what's on now, or what's next, and how fresh that answer is.

```jsx
<ContextTile kind="in_progress" title="Design review" timeLabel="9:30–10:00 AM" asOf="just now" />
<ContextTile kind="upcoming" title="School pickup" timeLabel="3:10–3:30 PM" asOf="42m ago" stale />
```

Staleness is never hidden: an old answer stays visible and turns amber rather than disappearing. A tile never mints or edits an action.
