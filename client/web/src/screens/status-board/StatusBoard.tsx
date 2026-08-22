import type { ReactNode } from "react";
import { useState } from "react";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { GithubPaneBody } from "../github-pane/GithubPaneExpanded";
import { KimiPaneBody } from "../kimi-pane/KimiPaneExpanded";
import type { QuestionInputs, RankedPane } from "../questions/contract";
import { QUESTION_ORDER } from "../questions/contract";
import { QUESTIONS, rankPanes } from "../questions/registry";
import { ReachabilityPaneBody } from "../reachability-pane/ReachabilityPaneExpanded";
import { reachabilityHasDetail } from "../reachability-pane/reachability";
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

/** Whether this pane has anything to disclose beneath its headline.
 *
 * Three of the four always do. Reachability usually does not — "Synced 12m
 * ago" is its whole answer — so it is drawn as a plain tile with no toggle
 * rather than one that opens onto an empty card. The predicate is the pane
 * module's own (`reachabilityHasDetail`), not a judgement made here. */
function hasDetail(pane: RankedPane, inputs: QuestionInputs): boolean {
  return pane.question === "reachability"
    ? reachabilityHasDetail(inputs)
    : true;
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

/** The tile's own control: a `button` when there is something to open, a
 * plain `div` when there is not. Both carry the tile's accessible name and
 * its test hooks, so a pane that cannot be opened is still announced and
 * still photographed — it simply has no `aria-expanded` to lie with. */
function Toggle({
  openable,
  open,
  detailId,
  label,
  tone,
  band,
  onToggle,
  children,
}: {
  openable: boolean;
  open: boolean;
  detailId: string;
  label: string;
  tone: TileTone;
  band: RankedPane["answer"]["band"];
  onToggle: () => void;
  children: ReactNode;
}) {
  const shared = {
    "aria-label": label,
    "data-tile-tone": tone,
    "data-band": band,
    className: open ? "hb-status-tile-button-open" : "hb-status-tile-button",
  } as const;
  if (!openable) {
    return (
      <div {...shared} role="group">
        {children}
      </div>
    );
  }
  return (
    <button
      type="button"
      aria-expanded={open}
      aria-controls={open ? detailId : undefined}
      onClick={onToggle}
      {...shared}
    >
      {children}
    </button>
  );
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
  const openable = hasDetail(pane, inputs);
  const open = expanded && openable;
  const detailId = `hb-status-detail-${pane.paneKey.replace(/[^a-zA-Z0-9-]/g, "-")}`;
  // The subject is worth announcing only when it is not the label said twice
  // — the same guard the detail line below uses.
  const spoken = name === definition.label ? fact : `${name} · ${fact}`;

  return (
    <Card
      padding={open ? "var(--space-5)" : "var(--space-4) var(--space-3)"}
      accent={open}
      className={`hb-status-tile${open ? " hb-status-tile-wide" : ""}`}
      style={
        // `Card` sets the `border` shorthand, so the ring has to replace it
        // wholesale: mixing shorthand and longhand on one element drops values
        // between renders (React warns about exactly this).
        ring === null ? undefined : { border: `1.5px solid ${ring}` }
      }
    >
      {/* One control per tile, and the only `aria-expanded` on the board.
          A pane with nothing beneath its headline gets a plain `div`
          instead: a disclosure control that discloses nothing reads as a
          broken one. */}
      <Toggle
        openable={openable}
        open={open}
        detailId={detailId}
        label={`${definition.label} — ${spoken}`}
        tone={tone}
        band={pane.answer.band}
        onToggle={onToggle}
      >
        {open ? (
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
      </Toggle>
      {open ? (
        <div className="hb-status-tile-detail" id={detailId}>
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

/** A group name as an id fragment. */
function slug(group: StatusGroup): string {
  return group.replace(/[^a-z]+/gi, "-").toLowerCase();
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
          <section
            key={group}
            aria-labelledby={`hb-status-group-${slug(group)}`}
          >
            {/* A heading, not a styled span: an unnamed `section` is not a
                landmark, so the two groups — the one piece of structure this
                board adds — were invisible to a screen reader, which heard
                ten tiles in a flat list. */}
            <h2 className="hb-meta" id={`hb-status-group-${slug(group)}`}>
              {group} · {subjectCount(members.length)}
            </h2>
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
