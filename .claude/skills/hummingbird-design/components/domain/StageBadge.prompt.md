Where an item sits in the funnel: Triage → Grilling → Ready → In Progress → Blocked → Done.

```jsx
<StageBadge stage="in_progress" />
<StageBadge stage="blocked" compact />
```

Stage colours are fixed tokens (`--stage-*`) — never recolour a stage per view. "Blocked" means an external wait only, never a dependency on another action.
