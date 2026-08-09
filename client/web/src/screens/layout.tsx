import type { ReactNode } from "react";

// The screen skeletons every surface shares. The design README fixes the
// rail and the right-hand panel and lets only the centre column scroll; the
// shell owns the scroll container, and `Aside` sticks to the top of it so
// the panel stays put while the column moves under it.

export function TwoColumn({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        display: "flex",
        gap: "var(--space-8)",
        alignItems: "flex-start",
        flexWrap: "wrap",
      }}
    >
      {children}
    </div>
  );
}

export function Column({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        flex: 1,
        minWidth: 380,
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-7)",
      }}
    >
      {children}
    </div>
  );
}

export function Aside({ children }: { children: ReactNode }) {
  return (
    <aside
      style={{
        width: "var(--panel-width)",
        flex: "0 0 auto",
        position: "sticky",
        top: 0,
        alignSelf: "flex-start",
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-6)",
      }}
    >
      {children}
    </aside>
  );
}

/** A single-column surface capped at the content measure. */
export function SingleColumn({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        maxWidth: "var(--content-max)",
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-7)",
      }}
    >
      {children}
    </div>
  );
}

export function Section({
  title,
  meta,
  children,
}: {
  title: string;
  meta?: string;
  children: ReactNode;
}) {
  return (
    <div>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          marginBottom: "var(--space-4)",
        }}
      >
        <h3 style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>{title}</h3>
        {meta ? <span className="hb-meta">{meta}</span> : null}
      </div>
      {children}
    </div>
  );
}
