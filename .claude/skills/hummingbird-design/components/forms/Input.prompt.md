Single-line text field with optional label, leading icon and hint/error line.

```jsx
<Input label="Capture" icon="feather" placeholder="What's on your mind?" />
<Input label="Due date" error="Due dates are deadlines — leave blank if nothing breaks." />
```

The capture field is the app's most-used input: give it `icon="feather"` and a plain-spoken placeholder.

The hint/error line is wired to the field: it carries an id derived from the field's own id, the input points `aria-describedby` at it whenever one is rendered, and `error` also sets `aria-invalid`. Pass an `id` and it takes over both ends of that association. `onFocus`/`onBlur` are composed, not replaced — the internal handler drives the focus ring first, then yours runs.
