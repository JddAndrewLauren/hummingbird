// PROTOTYPE — throwaway. Delete with the rest of `now-prototype/`.
//
// The facet chip row, shared by variant A (always on screen) and variant C
// (behind a Filter button). Shared because it is chrome both variants agree
// about — they disagree about whether it should be *visible*, which is the
// interesting difference, not how a chip is drawn.

export function Chip({
  label,
  active,
  onClick,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      style={{
        font: "var(--type-body-sm)",
        padding: "var(--space-2) var(--space-4)",
        borderRadius: 999,
        border: `1px solid ${active ? "var(--accent-quiet-border)" : "var(--border-default)"}`,
        background: active ? "var(--accent-quiet)" : "transparent",
        color: active ? "var(--text-brand)" : "var(--text-secondary)",
        cursor: "pointer",
        transition: "background var(--dur-fast) var(--ease-flit)",
      }}
    >
      {label}
    </button>
  );
}

export function FacetRow({
  title,
  values,
  selected,
  onToggle,
}: {
  title: string;
  values: readonly string[];
  selected: ReadonlySet<string>;
  onToggle: (value: string) => void;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
      <span className="hb-meta" style={{ width: 62, flex: "0 0 auto" }}>
        {title}
      </span>
      <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
        {values.map((value) => (
          <Chip
            key={value}
            label={value}
            active={selected.has(value)}
            onClick={() => onToggle(value)}
          />
        ))}
      </div>
    </div>
  );
}
