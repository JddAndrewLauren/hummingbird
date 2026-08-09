A settings toggle: label left, track right, for things that take effect immediately.

```jsx
<Switch checked={urgentOnly} onChange={toggle} label="Urgent tier only"
  hint="Normal-tier deliveries stay silent on this device." />
```

Use it for device/notification preferences; use Checkbox for list selection.
