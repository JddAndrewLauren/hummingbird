// PROTOTYPE — throwaway. Delete with the rest of this directory (#120).
//
// A — "Kerb tile". Two renderings of one answer, chosen by
// `PaneView.prominence`:
//
//   dormant   — a single quiet line, no card at all, so it cannot compete
//               with the frontier. Coloured dots keep the bins identifiable
//               without spending any vertical space.
//   prominent — the eve, the day itself, or a holiday week. The full tile,
//               `accent`, three words and a date.
//
// There is no AlertCard here any more: a holiday is not an interruption laid
// over the answer, it IS the answer, so it changes the words rather than
// stacking a card above them. The alert row is still minted for the
// notification lane (`mint` in `waste.ts`) — this pane simply does not join
// to it.
//
// Almost all the text is gone on purpose. The bins carry their real colours
// (grey / light blue / green), so the words only have to say *when* — see
// `BIN` in `waste.ts` for why that colour use is a deliberate exception to
// the design system's "colour always encodes status" rule. The bins keep
// accessible names, because colour alone is not a label to a screen reader.

import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { EmptyState } from "../../components/feedback/EmptyState";
import { BIN, STREAM_ORDER, shortDate, weekday } from "./waste";
import type { PaneView, Stream } from "./waste";

export const NAME = "Kerb tile";
export const SLOT = "aside";

/** Always in kerb order, never in the order the payload happened to list. */
function ordered(streams: Stream[]): Stream[] {
  return STREAM_ORDER.filter((s) => streams.includes(s));
}

function Bin({ stream }: { stream: Stream }) {
  const bin = BIN[stream];
  return (
    <span
      role="img"
      aria-label={bin.label}
      style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 3 }}
    >
      <span
        style={{ width: 34, height: 5, borderRadius: "var(--radius-sm)", background: bin.edge }}
      />
      <span
        style={{
          width: 30,
          height: 38,
          borderRadius: "var(--radius-sm)",
          border: `1px solid ${bin.edge}`,
          background: bin.fill,
        }}
      />
    </span>
  );
}

function Dot({ stream }: { stream: Stream }) {
  return (
    <span
      role="img"
      aria-label={BIN[stream].label}
      style={{
        width: 8,
        height: 8,
        borderRadius: "50%",
        background: BIN[stream].fill,
        border: `1px solid ${BIN[stream].edge}`,
        flex: "0 0 auto",
      }}
    />
  );
}

/** Most of the week. One line, muted, no card — it has to read as furniture,
 * because on those days it is. */
function Dormant({ view }: { view: PaneView }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: "var(--space-3)",
        font: "var(--type-meta)",
        letterSpacing: "var(--tracking-meta)",
        textTransform: "uppercase",
        color: "var(--text-muted)",
      }}
    >
      {ordered(view.streams).map((stream) => (
        <Dot key={stream} stream={stream} />
      ))}
      <span>
        {weekday(view.collectionDate)} · {view.daysAway}d
      </span>
    </div>
  );
}

function Tile({ view }: { view: PaneView }) {
  return (
    <Card
      accent
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
    >
      <div style={{ display: "flex", gap: "var(--space-4)", justifyContent: "center" }}>
        {ordered(view.streams).map((stream) => (
          <Bin key={stream} stream={stream} />
        ))}
      </div>

      <p
        style={{
          font: "var(--weight-bold) 24px/1.1 var(--font-display)",
          letterSpacing: "var(--tracking-heading)",
          color: "var(--text-primary)",
        }}
      >
        {view.headline}
      </p>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-3)",
          flexWrap: "wrap",
        }}
      >
        <span
          style={{
            font: "var(--type-meta)",
            letterSpacing: "var(--tracking-meta)",
            textTransform: "uppercase",
            color: "var(--text-muted)",
          }}
        >
          {shortDate(view.collectionDate)}
        </span>
        {view.holiday ? (
          <Badge tone="warn" mono>
            holiday
          </Badge>
        ) : null}
        {/* Staleness is shown only once it matters. The poll is daily, so
            anything past a day means the answer may be wrong — and the brand
            rule is to keep showing the data and say its age, never to hide
            it. On a fresh snapshot this line would be pure noise. */}
        {view.staleMinutes > 26 * 60 ? (
          <span
            style={{
              font: "var(--type-meta)",
              letterSpacing: "var(--tracking-meta)",
              textTransform: "uppercase",
              color: "var(--status-warn-fg)",
            }}
          >
            stale — as of {Math.floor(view.staleMinutes / 60)}h ago
          </span>
        ) : null}
      </div>
    </Card>
  );
}

export function VariantA({ view }: { view: PaneView }) {
  return (
    <div>
      <span className="hb-meta">which cans</span>
      <div style={{ marginTop: "var(--space-4)" }}>
        {view.streams.length === 0 ? (
          <Card padding="var(--space-3)">
            <EmptyState
              compact
              icon="calendar-clock"
              headingLevel={2}
              title="No collection data"
              body="The waste poll has not landed a snapshot."
            />
          </Card>
        ) : view.prominence === "dormant" ? (
          <Dormant view={view} />
        ) : (
          <Tile view={view} />
        )}
      </div>
    </div>
  );
}
