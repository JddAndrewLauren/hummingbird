A pushed alert in the notification lane: source identity, severity tier, link back, and the Ack gesture.

```jsx
<AlertCard tier="urgent" source="Fly · hb-worker" title="Sweeper run failed"
  detail="Google Tasks adapter returned 503 twice." time="6m" href="#" onAck={ack} />
```

Acking is the only thing that clears an alert from the working view; dismissing a notification is not an ack. Alerts never become actions except by a human gesture.
