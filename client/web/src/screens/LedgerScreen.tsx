import { Badge } from "../components/core/Badge";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import { relativeAge } from "../shell/sync-status";
import type { LedgerRowDTO } from "../store/protocol";
import type { TaskState } from "../store/store";
import { Section, SingleColumn } from "./layout";
import { ledgerRowState, lastTouchedMs, orderLedger } from "./ledger-order";

export interface LedgerScreenProps {
  task: TaskState;
  nowMs: number;
}

/** One roster row: stage, title, state label, last-touched age and the three
 * derivable badges. Read-only by decision — every mutation stays on
 * Now/Triage — so this is a plain row, not a button, and offers nothing. */
function LedgerRow({ row, nowMs }: { row: LedgerRowDTO; nowMs: number }) {
  const state = ledgerRowState(row);
  return (
    <Card
      padding="var(--space-5)"
      style={{
        display: "flex",
        alignItems: "center",
        gap: "var(--space-5)",
        flexWrap: "wrap",
        // An archived row stays readable but reads as settled history —
        // recede, never hide ("each labelled with its state").
        opacity: state.kind === "archived" ? 0.72 : 1,
      }}
    >
      <StageBadge stage={row.stage} />
      <span
        style={{
          flex: "1 1 220px",
          minWidth: 0,
          font: "var(--type-body)",
          color: "var(--text-primary)",
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          textDecoration: state.kind === "archived" ? "line-through" : "none",
        }}
      >
        {row.title}
      </span>
      {row.pending ? (
        <span
          title="Not yet confirmed by the server"
          className="hb-meta"
          style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", flex: "0 0 auto" }}
        >
          <Icon name="loader-circle" size={13} />
          Pending
        </span>
      ) : null}
      {row.deadLettered ? (
        <Badge mono tone="danger" title="A dead-lettered edit targets this item — on this device">
          edit didn't apply
        </Badge>
      ) : null}
      {row.hasLiveAlert ? (
        <Badge mono tone="warn" icon="bell">
          alert
        </Badge>
      ) : null}
      <span className="hb-meta" style={{ flex: "0 0 auto", whiteSpace: "nowrap" }}>
        {state.kind === "archived"
          ? `archived ${relativeAge(Math.max(0, nowMs - state.sinceMs))}`
          : `touched ${relativeAge(Math.max(0, nowMs - lastTouchedMs(row)))}`}
      </span>
    </Card>
  );
}

/** The Ledger: the complete retained roster — every item the mirror has ever
 * known, live, Done and archived alike, last touched first.
 *
 * Derive, don't record (the grilling that shaped this screen): no transition
 * history exists anywhere, so a row states current facts and never claims a
 * chronicle. Ordering and labels are `ledger-order.ts`'s; this component
 * threads store state through them. `task.ledger` is `null` until the first
 * answer arrives, and that renders as "not read yet" — an empty roster is a
 * claim only a loaded core may make. */
export function LedgerScreen({ task, nowMs }: LedgerScreenProps) {
  const rows = task.ledger;
  return (
    <SingleColumn>
      <Section
        title="Every item"
        meta={rows === null ? "not read yet" : `${rows.length} ever · derived, not recorded`}
      >
        {rows === null ? (
          <Card padding="var(--space-6)">
            <span className="hb-meta">Waiting for the core's first answer.</span>
          </Card>
        ) : rows.length === 0 ? (
          <Card padding="0">
            <EmptyState
              icon="scroll-text"
              headingLevel={3}
              title="Nothing has ever been tracked"
              body="Every item the mirror has ever known appears here — archived ones included, labelled."
            />
          </Card>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            {orderLedger(rows).map((row) => (
              <LedgerRow key={row.id} row={row} nowMs={nowMs} />
            ))}
          </div>
        )}
      </Section>
    </SingleColumn>
  );
}
