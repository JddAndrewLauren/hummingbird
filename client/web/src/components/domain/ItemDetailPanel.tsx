import { useState } from "react";
import { Badge } from "../core/Badge";
import { Button } from "../core/Button";
import { Card } from "../core/Card";
import { IconButton } from "../core/IconButton";
import { Checkbox } from "../forms/Checkbox";
import { Select } from "../forms/Select";
import { availableActions } from "../../screens/item-actions";
import { hasPriority, priorityLabel } from "../../screens/priority";
import { microtaskAffordance } from "../../skills/microtask-affordance";
import { IDLE, isRunning, stampLabel, type SkillRunState } from "../../skills/run-state";
import type { MicrotaskRunRequest } from "../../shell/useMicrotaskWiring";
import type { StepDTO, TaskActionName, TaskItemDTO } from "../../store/protocol";
import { StageBadge } from "./StageBadge";

/** One `TaskActionName`'s button label and leading icon — voice per the
 * design README ("Blocked" means an external wait and nothing else, so its
 * label says so rather than leaving the word to speak for itself). */
const ACTION_BUTTON: Record<TaskActionName, { label: string; icon: "play" | "check" | "clock" | "x" }> = {
  start: { label: "Start", icon: "play" },
  complete: { label: "Complete", icon: "check" },
  block: { label: "Mark blocked", icon: "clock" },
  cancel: { label: "Cancel", icon: "x" },
};

export interface ItemDetailPanelProps {
  item: TaskItemDTO;
  /** Whatever `TaskState.stepsByItem[item.id]` currently holds — `[]`
   * until the request answers, same "not yet known" shape every other S9
   * read uses; there is no separate loading flag. */
  steps: StepDTO[];
  onClose: () => void;
  /** S11/#109's act affordances. Omitted (no action row rendered) for a
   * caller that only wants the read-only panel — every existing call site
   * before this slice passed none. */
  onAct?: (action: TaskActionName) => void;
  /** Set whenever the most recent act request for THIS item resolved to
   * anything other than `"ok"` (`TaskState.lastAct`, matched by the caller
   * on `itemId`) — reviewer finding on PR #207: a failed act used to be
   * recorded in the store and rendered nowhere, silently. Cleared by the
   * caller once a fresh action is taken (a new attempt speaks for itself,
   * per this product's "state what is true and stop" voice — an old error
   * next to a brand-new pending badge would be confusing, not honest). */
  actError?: string | null;
  /** #273's microtask affordance. Optional, the `onSetScheduledDate`
   * precedent: `undefined` in demo mode, which is what guarantees a future
   * demo detail view cannot issue a real request. (There is no demo path to
   * build today — `NowScreen` branches to `RealFrontier` only when demo is
   * off, so this panel is never mounted under `?demo`.) */
  microtask?: {
    run: SkillRunState;
    onRun: (request: MicrotaskRunRequest) => void;
    /** #274's pinned-decline affordance: set only while `run.phase` is
     * `"declined"` AND the current selection is a pin (never Auto) AND the
     * registry has another entry to offer. One tap both switches and
     * retries, but as a **single call** the caller owns: this panel hands
     * over the request it built the first time and nothing else. Switching
     * here and then calling `onRun` would retry against the selection this
     * render closed over — the pin that just declined. */
    declinedFallback?: {
      label: string;
      onSwitchAndRun: (request: MicrotaskRunRequest) => void;
    } | null;
  };
}

/** SKILL.md's grain scale, as the select renders it. */
const GRAINS = [
  { value: "1", label: "Coarse" },
  { value: "2", label: "Default grain" },
  { value: "3", label: "Fine" },
];

/** Item detail: description and Steps (issue #96), read-only from this
 * binding — S11 wires ticking a Step. Priority renders by its label, never
 * the raw wire number (`priorityLabel`), same rule the frontier list's
 * ordering applies.
 *
 * The action row (S11/#109) offers exactly what `availableActions`
 * (`screens/item-actions.ts`) says for `item.stage` — there is no
 * "depends on another action" affordance anywhere here: that is a
 * `blocked_by` relation edge (S10's `getBlocked`), never expressible
 * through this panel's `"block"` button, which only ever sets the funnel's
 * `Blocked` stage (an external wait). Every button disables itself while
 * `item.pending` — a second mutation queued on top of an unconfirmed one
 * would only add a redundant request, never corrupt anything, but there is
 * nothing useful for a person to do with the item until the first one
 * resolves. */
export function ItemDetailPanel({ item, steps, onClose, onAct, actError = null, microtask }: ItemDetailPanelProps) {
  // Local, and reset per item by the `key` on this element in
  // `RealFrontier`: a grain chosen for one item says nothing about the next.
  const [grain, setGrain] = useState("2");
  const run = microtask?.run ?? IDLE;
  const affordance = microtaskAffordance(steps);
  const running = isRunning(run);
  const stamp = stampLabel(run);
  // Which backend answers is an app-level preference now (#274's picker,
  // Settings) — never chosen here, and never varied per item or per skill.
  const runRequest: MicrotaskRunRequest =
    affordance.kind === "break"
      ? { itemId: item.id }
      : { itemId: item.id, replace: true, grain: Number(grain) };

  return (
    <Card
      elevation={2}
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
          <span className="hb-meta">{item.seq !== null ? `HB-${item.seq}` : "item detail"}</span>
          <h2 style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>{item.title}</h2>
        </div>
        <IconButton icon="x" label="Close item detail" onClick={onClose} />
      </div>

      <div style={{ display: "flex", gap: "var(--space-3)", flexWrap: "wrap" }}>
        <StageBadge stage={item.stage} />
        {hasPriority(item.priority) ? <Badge tone="brand">{priorityLabel(item.priority)}</Badge> : null}
        {item.size ? <Badge mono>size:{item.size}</Badge> : null}
        {item.energy ? <Badge mono>energy:{item.energy}</Badge> : null}
        {item.context ? <Badge mono>{item.context}</Badge> : null}
      </div>

      {onAct && availableActions(item.stage).length > 0 ? (
        <div style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
          {availableActions(item.stage).map((action, index) => {
            const { label, icon } = ACTION_BUTTON[action];
            return (
              <Button
                key={action}
                variant={index === 0 ? "primary" : action === "cancel" ? "ghost" : "secondary"}
                iconLeft={icon}
                disabled={item.pending}
                onClick={() => onAct(action)}
              >
                {label}
              </Button>
            );
          })}
        </div>
      ) : null}

      {/* `role="alert"` on every one of this app's danger-text paragraphs
          (here, both of `TriageScreen.tsx`'s, and `SettingsScreen.tsx`'s):
          each renders only once a write has already failed, so it appears
          with no other change on the page and a screen reader would
          otherwise never announce it. Colour was the whole signal. */}
      {actError ? (
        <p role="alert" style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)" }}>{actError}</p>
      ) : null}

      {item.description ? (
        <p style={{ font: "var(--type-body)", color: "var(--text-secondary)" }}>{item.description}</p>
      ) : null}

      {/* The microtask affordance belongs to the steps block, not the act
          row above: that row is `availableActions(item.stage)` — the funnel
          — and asking for a checklist moves the item through nothing. */}
      <div>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "var(--space-4)",
            flexWrap: "wrap",
          }}
        >
          <span className="hb-meta">steps</span>
          {microtask ? (
            <div style={{ display: "flex", alignItems: "flex-end", gap: "var(--space-4)", flexWrap: "wrap" }}>
              {affordance.kind === "rewrite" ? (
                <Select
                  label="Grain"
                  size="sm"
                  options={GRAINS}
                  value={grain}
                  onChange={(event) => setGrain(event.target.value)}
                />
              ) : null}
              <Button
                variant="secondary"
                size="sm"
                iconLeft={affordance.kind === "break" ? "sparkles" : "rotate-ccw"}
                loading={running}
                onClick={() => microtask.onRun(runRequest)}
              >
                {/* The count is what makes a rewrite's destructive half
                    legible without a confirm dialog. Not `variant="danger"`:
                    it is a rewrite the user asked for, and the ticked steps
                    are untouched. */}
                {affordance.kind === "break"
                  ? "Break into steps"
                  : `Rewrite ${affordance.undoneCount} step${affordance.undoneCount === 1 ? "" : "s"}`}
              </Button>
            </div>
          ) : null}
        </div>
        {steps.length === 0 ? (
          <p
            style={{
              font: "var(--type-body-sm)",
              color: "var(--text-muted)",
              marginTop: "var(--space-3)",
            }}
          >
            No Steps yet.
          </p>
        ) : (
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: "var(--space-4)",
              marginTop: "var(--space-3)",
            }}
          >
            {steps.map((step) => (
              <Checkbox key={step.id} checked={step.done} label={step.body} />
            ))}
          </div>
        )}

        {run.phase !== "idle" ? (
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: "var(--space-3)",
              marginTop: "var(--space-5)",
            }}
          >
            {/* `role="status"` (polite), the deliberate counterpart of the
                `role="alert"` this file already justifies for `actError`:
                narration arrives line by line while the user is watching,
                so it must not interrupt. */}
            <div role="status" style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
              {run.messages.map((message, index) => (
                <span
                  key={`${index}-${message}`}
                  style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}
                >
                  {message}
                </span>
              ))}
            </div>

            {stamp ? <div><Badge mono>{stamp}</Badge></div> : null}

            {run.phase === "done" && run.note ? (
              <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>{run.note}</p>
            ) : null}

            {/* Verbatim, unprefixed, unbranched — #307 made the seam's
                decline prose-only with no reason code precisely so nothing
                string-matches it. */}
            {run.phase === "declined" ? (
              <>
                <p role="alert" style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)" }}>
                  {run.reason}
                </p>
                {/* #274: a pinned, dead backend is never silently rerouted —
                    this is the one-tap offer, not an automatic fallback.
                    Absent whenever the current selection is Auto (nothing to
                    fall back FROM) or the registry has nothing else to try. */}
                {microtask?.declinedFallback ? (
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => microtask.declinedFallback?.onSwitchAndRun(runRequest)}
                  >
                    Switch to {microtask.declinedFallback.label}
                  </Button>
                ) : null}
              </>
            ) : null}
          </div>
        ) : null}
      </div>
    </Card>
  );
}
