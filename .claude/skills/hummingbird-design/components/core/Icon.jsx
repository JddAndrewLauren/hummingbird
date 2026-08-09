import React from "react";

// Lucide is the icon set (loaded from CDN as `window.lucide`). See
// readme.md ICONOGRAPHY — the source repo ships no icon set of its own.
export function Icon({ name, size = 18, strokeWidth = 1.75, color = "currentColor", title, style = {}, ...rest }) {
  const ref = React.useRef(null);
  React.useEffect(() => {
    const host = ref.current;
    if (!host) return;
    let cancelled = false;
    const draw = () => {
      if (cancelled || !host || !window.lucide) return;
      host.innerHTML = "";
      const i = document.createElement("i");
      i.setAttribute("data-lucide", name);
      host.appendChild(i);
      window.lucide.createIcons({ attrs: { "stroke-width": strokeWidth } });
    };
    if (window.lucide) draw();
    else {
      const t = setInterval(() => { if (window.lucide) { clearInterval(t); draw(); } }, 60);
      setTimeout(() => clearInterval(t), 4000);
      return () => { cancelled = true; clearInterval(t); };
    }
    return () => { cancelled = true; };
  }, [name, strokeWidth]);
  return (
    <span ref={ref} aria-hidden={title ? undefined : true} aria-label={title} role={title ? "img" : undefined}
      style={{ display: "inline-flex", alignItems: "center", justifyContent: "center", width: size, height: size, color, flex: "0 0 auto", ...style }}
      {...rest} />
  );
}
