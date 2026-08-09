One Lucide glyph, sized and stroked to Hummingbird's defaults; use it anywhere an icon appears rather than inlining SVG.

```jsx
<Icon name="bell" size={18} />
<Icon name="calendar-clock" size={20} color="var(--accent)" title="Calendar context" />
```

Requires the Lucide UMD script on the page (`https://unpkg.com/lucide@latest/dist/umd/lucide.js`). Stroke width is 1.75 everywhere; drop to 1.5 only above 32px.
