A discrete slider for optional capture metadata — energy and size. Unset is the default and a fine place to stay.

```jsx
<Slider label="Energy" options={["low","medium","high"]} value={energy} onChange={setEnergy} />
<Slider label="Size" options={["quick","normal","deep"]} value={size} onChange={setSize} />
```

Click a stop label, the track, or drag; × clears back to "not set". Never make a slider required at capture — deciding is mint-time work.

From the track: ArrowRight/ArrowUp step up, ArrowLeft/ArrowDown step down, Home and End jump to the first and last option. All six consume the key, so adjusting the value never scrolls the page. Arrows never clear — the × does that. Each stop label is a button in the tab order too, so there is a second keyboard path that names its target.

Unset reports `aria-valuenow="-1"`, a sentinel one below the first stop and inside the declared range (`aria-valuemin="-1"`); `role="slider"` requires the property, and this keeps "not set" from reading as a deliberate pick of the lowest option. `aria-valuetext` says "not set" or the option label.
