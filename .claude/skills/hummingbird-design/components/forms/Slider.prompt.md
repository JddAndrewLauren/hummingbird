A discrete slider for optional capture metadata — energy and size. Unset is the default and a fine place to stay.

```jsx
<Slider label="Energy" options={["low","medium","high"]} value={energy} onChange={setEnergy} />
<Slider label="Size" options={["quick","normal","deep"]} value={size} onChange={setSize} />
```

Click a stop label, the track, or drag; × clears back to "not set". Never make a slider required at capture — deciding is mint-time work.
