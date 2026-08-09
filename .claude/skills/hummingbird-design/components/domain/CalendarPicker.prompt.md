Which context calendars this device polls. One checkbox per calendar; the selection is per-device.

```jsx
<CalendarPicker calendars={cals} selectedIds={ids}
  unavailableIds={["team@group.calendar.google.com"]} onToggle={toggle} />
```

Never silently drop a selected-but-missing calendar — list it amber so unchecking is a deliberate act.
