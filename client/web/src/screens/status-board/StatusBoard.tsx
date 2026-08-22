import { useState } from "react";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { GithubPaneBody } from "../github-pane/GithubPaneExpanded";
import { KimiPaneBody } from "../kimi-pane/KimiPaneExpanded";
import type { QuestionInputs, RankedPane } from "../questions/contract";
import { QUESTION_ORDER } from "../questions/contract";
import { QUESTIONS, rankPanes } from "../questions/registry";
import { ReachabilityPaneBody } from "../reachability-pane/ReachabilityPaneExpanded";
import type { StorageLike } from "../storage";
import { UptimePaneBody } from "../uptime-pane/UptimePaneExpanded";
import {
  bandWord,
  subjectCount,
  tileParts,
  tileTone,
  type TileTone,
} from "./tile-copy";
import { readExpandedKey, writeExpandedKey } from "./status-prefs";
import {
  STATUS_GROUPS,
  tileGroup,
  tileIcon,
  type StatusGroup,
} from "./tile-vocabulary";
import type { SyncStatusInput } from "../../shell/sync-status";
import {
  relativeAge,
  syncStatusLabel,
  syncStatusToneWord,
} from "../../shell/sync-status";

// The Status board (the design handoff's "expanding tiles"): the same panes
// `rankPanes(…, "status")` has always decided, drawn as two labelled grids
// of tiles instead of a stack of rows. Clicking a tile expands *it*, in
// place, across two columns.
//
// **Why this is not `RankedRegion`.** That component's captured sample
// exists to stop salience order sliding a row out from under a reaching
// cursor: it freezes position while letting content stay live. A board does
// not need it, because position here is a function of *identity* — the group,
// then the declared question order, then the subject — so a band change
// cannot move a tile at all. That is a stronger guarantee than the sample,
// and it costs no state. `rankPanes` remains the only decision source
// (ADR-0025); grouping and ordering for layout is rendering, and so is which
// of two words a group gets (`tile-vocabulary.ts`).
//
// The consequence worth stating: this surface no longer has a per-band
// collapse override, because it no longer has per-pane collapse. What the
// reader chose is one open tile (`status-prefs.ts`), and a problem announces
// itself by its treatment rather than by opening itself. `collapse.ts` is
// not reimplemented here — it is simply not used, and Now still owns it.
//
// **There is no loading arm, deliberately.** ADR-0015's rule is that a
// surface must never render an empty list pretending to be all quiet — and on
// this surface the panes themselves keep it: no status question has a binding
// to be unbound from, so before anything has been polled every one of them
// answers `bound-but-unacquired` and says so in words (`No answer yet`, and
// its own reason). Those gap tiles, which carry no green dot, ARE the honest
// first frame. A spinner in front of them would hide real answers behind a
// state this surface is never actually in.

export interface StatusBoardProps {
  /** Everything the questions answer from, minus the clock. */
  inputs: Omit<QuestionInputs, "nowMs">;
  nowMs: number;
  online: boolean;
  queueDepth: number | null;
  lastSyncOutcome: SyncStatusInput["lastSyncOutcome"];
  lastSyncAtMs: number | null;
  storage?: StorageLike;
}

/** The four pane bodies, by question. Each is the pane's own expanded
 * rendering minus the `Card` it draws for Now — the tile is already one.
 * An unregistered question renders no body; its tile still draws, with its
 * headline and facts, which is the graceful arm `tile-vocabulary.ts`
 * describes. */
function PaneBody({
  pane,
  inputs,
}: {
  pane: RankedPane;
  inputs: QuestionInputs;
}) {
  switch (pane.question) {
    case "kimi":
      return <KimiPaneBody inputs={inputs} headline={false} />;
    case "github":
      return (
        <GithubPaneBody
          subjectKey={pane.subjectKey}
          inputs={inputs}
          headline={false}
        />
      );
    case "uptime":
      return (
        <UptimePaneBody
          subjectKey={pane.subjectKey}
          inputs={inputs}
          headline={false}
        />
      );
    case "reachability":
      return <ReachabilityPaneBody inputs={inputs} headline={false} />;
    default:
      return null;
  }
}

const TONE_COLOR: Record<TileTone, string> = {
  quiet: "var(--text-secondary)",
  warn: "var(--status-warn-fg)",
  danger: "var(--status-danger-fg)",
  gap: "var(--text-secondary)",
};

/** The 1.5px band-coloured ring a problem tile wears, at 75% so it reads as
 * a tint on the card's own border rather than a second border. */
function ringColor(tone: TileTone): string | null {
  if (tone === "warn")
    return "color-mix(in oklab, var(--status-warn-fg) 75%, transparent)";
  if (tone === "danger")
    return "color-mix(in oklab, var(--status-danger-fg) 75%, transparent)";
  return null;
}

function StatusTile({
  pane,
  inputs,
  expanded,
  onToggle,
}: {
  pane: RankedPane;
  inputs: QuestionInputs;
  expanded: boolean;
  onToggle: () => void;
}) {
  const definition = QUESTIONS[pane.question];
  const { name, fact } = tileParts(
    definition.label,
    pane.answer.collapsedHeadline,
  );
  const tone = tileTone(pane.answer);
  const ring = ringColor(tone);
  const icon = tileIcon(pane);

  return (
    <Card
      padding={expanded ? "var(--space-5)" : "var(--space-4) var(--space-3)"}
      accent={expanded}
      className={`hb-status-tile${expanded ? " hb-status-tile-wide" : ""}`}
      style={
        // `Card` sets the `border` shorthand, so the ring has to replace it
        // wholesale: mixing shorthand and longhand on one element drops values
        // between renders (React warns about exactly this).
        ring === null ? undefined : { border: `1.5px solid ${ring}` }
      }
    >
      {/* One button per tile, and the only `aria-expanded` on the board —
          the whole tile is the toggle, so the compact form has no separate
          hit target and the expanded form has no second one. */}
      <button
        type="button"
        aria-expanded={expanded}
        aria-label={`${definition.label} — ${name} · ${fact}`}
        data-tile-tone={tone}
        data-band={pane.answer.band}
        onClick={onToggle}
        className={
          expanded ? "hb-status-tile-button-open" : "hb-status-tile-button"
        }
      >
        {expanded ? (
          <span className="hb-status-tile-head">
            <Icon name={icon} size={18} color={TONE_COLOR[tone]} />
            <span
              className="hb-status-tile-headline"
              style={{ color: TONE_COLOR[tone] }}
            >
              {/* The open tile says the pane's whole decided sentence — the
                  copy the handoff itself quotes (`runner · unreachable —
                  connect timeout`). The body under it draws its supporting
                  detail only, never this line again. */}
              {pane.answer.collapsedHeadline}
            </span>
            <span className="hb-meta hb-status-tile-band">
              {bandWord(pane.answer.band)}
            </span>
          </span>
        ) : (
          <>
            <Icon name={icon} size={24} color={TONE_COLOR[tone]} />
            <span className="hb-status-tile-name">{name}</span>
            <span
              className="hb-meta hb-status-fact"
              style={
                tone === "quiet" || tone === "gap"
                  ? undefined
                  : { color: TONE_COLOR[tone] }
              }
            >
              {fact}
            </span>
            {/* A green dot means "answered, and as expected". A gap gets
                none: it has no answer to call expected. */}
            {tone === "quiet" ? <span className="hb-status-dot" /> : null}
          </>
        )}
      </button>
      {expanded ? (
        <div className="hb-status-tile-detail">
          {/* The full pane name, to say which subject the headline is about.
              A question with one subject has nothing to disambiguate — its
              tile is already named by its label — and "Kimi balance —
              balance" is the subject saying the label back. */}
          <span className="hb-meta">
            {name === definition.label
              ? definition.label
              : `${definition.label} — ${pane.subjectKey}`}
          </span>
          <PaneBody pane={pane} inputs={inputs} />
        </div>
      ) : null}
    </Card>
  );
}

/** Layout order: the declared question order, then the subject. Identity
 * only — never the band, so nothing moves under the cursor. */
function byIdentity(a: RankedPane, b: RankedPane): number {
  const questionDelta =
    QUESTION_ORDER.indexOf(a.question) - QUESTION_ORDER.indexOf(b.question);
  return questionDelta !== 0
    ? questionDelta
    : a.subjectKey.localeCompare(b.subjectKey);
}

export function StatusBoard({
  inputs,
  nowMs,
  online,
  queueDepth,
  lastSyncOutcome,
  lastSyncAtMs,
  storage,
}: StatusBoardProps) {
  const [expandedKey, setExpandedKey] = useState<string | null>(() =>
    readExpandedKey(storage),
  );

  function toggle(paneKey: string): void {
    const next = expandedKey === paneKey ? null : paneKey;
    setExpandedKey(next);
    writeExpandedKey(storage, next);
  }

  const liveInputs: QuestionInputs = { ...inputs, nowMs };
  const syncInput: SyncStatusInput = {
    online,
    lastSyncOutcome,
    lastSyncAtMs,
    queueDepth,
    nowMs,
  };

  const strip = (
    <Card padding="var(--space-4) var(--space-5)">
      <div className="hb-status-strip">
        <Icon name="refresh-cw" size={16} color="var(--text-secondary)" />
        <span style={{ font: "var(--type-body-sm)" }}>
          {syncStatusLabel(syncInput)}
        </span>
        <span className="hb-meta hb-status-strip-tone">
          {syncStatusToneWord(syncInput)}
        </span>
      </div>
    </Card>
  );

  const panes = rankPanes(liveInputs, "status");
  const grouped = new Map<StatusGroup, RankedPane[]>();
  for (const pane of panes) {
    const group = tileGroup(pane);
    const existing = grouped.get(group);
    if (existing) {
      existing.push(pane);
    } else {
      grouped.set(group, [pane]);
    }
  }

  return (
    <>
      <div className="hb-status-header">
        <span className="hb-meta">
          as of{" "}
          {lastSyncAtMs === null ? "never" : relativeAge(nowMs - lastSyncAtMs)}
        </span>
      </div>
      {strip}
      {STATUS_GROUPS.map((group) => {
        const members = grouped.get(group);
        // A group with nothing in it draws nothing at all — no label over an
        // empty grid.
        if (!members || members.length === 0) return null;
        return (
          <section key={group}>
            <span className="hb-meta">
              {group} · {subjectCount(members.length)}
            </span>
            <div
              className={`hb-status-grid ${group === "infra" ? "hb-status-grid-4" : "hb-status-grid-5"}`}
            >
              {[...members].sort(byIdentity).map((pane) => (
                <StatusTile
                  key={pane.paneKey}
                  pane={pane}
                  inputs={liveInputs}
                  expanded={expandedKey === pane.paneKey}
                  onToggle={() => toggle(pane.paneKey)}
                />
              ))}
            </div>
          </section>
        );
      })}
    </>
  );
}
