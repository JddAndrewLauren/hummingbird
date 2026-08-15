import { useState } from "react";
import { Badge } from "../core/Badge";
import { Button } from "../core/Button";
import { Card } from "../core/Card";
import { Icon } from "../core/Icon";
import { IconButton } from "../core/IconButton";
import { Checkbox } from "../forms/Checkbox";
import { Combobox } from "../forms/Combobox";
import { DeadlineField } from "../forms/DeadlineField";
import { Input } from "../forms/Input";
import { Select } from "../forms/Select";
import { Textarea } from "../forms/Textarea";
import {
  CONTEXTS,
  ENERGY_OPTIONS,
  SIZE_OPTIONS,
} from "../../screens/field-vocabulary";
import { availableActions, canGrill } from "../../screens/item-actions";
import { hasPriority, priorityLabel, PRIORITY_OPTIONS } from "../../screens/priority";
import {
  energyIcon,
  energyLabel,
  levelColor,
  sizeIcon,
  sizeLabel,
} from "../../screens/size-energy";
import { buildTriageEdits, hasTriageEdits } from "../../screens/triage-form";
import { useItemDraft } from "../../screens/useItemDraft";
import { triageFailureFor } from "../../screens/write-failure";
import { microtaskAffordance } from "../../skills/microtask-affordance";
import { IDLE, isRunning, stampLabel, type SkillRunState } from "../../skills/run-state";
import type { MicrotaskRunRequest } from "../../shell/useMicrotaskWiring";
import type {
  ProjectDTO,
  StepDTO,
  TaskActionName,
  TaskItemDTO,
} from "../../store/protocol";
import type { TaskTriageResult } from "../../store/store";
import type { TriageEdits } from "../../store/worker-client";
import { StageBadge } from "./StageBadge";

// **One item panel, two modes.** This is the whole of what an item looks like
// when it is the thing on screen — on Now, where a selected card expands into
// it, and on Triage, where a row opens into it. The two used to be separate
// components with separate field grids, and the divergence was not cosmetic:
// only one of them could edit an item, so a minted action's description was
// reachable nowhere in the app once it had left the inbox.
//
// The modes differ in what an item at that stage *affords*, never in how a
// field is drawn:
//
// - `"triage"` — the capture is unsorted, so the fields are the point and
//   stand open, and the row ends in the promotion to Ready plus Grill me
//   (#360: Ready is triage's only destination — an item reaches Grilling
//   exactly one way, a `fog_remains` Grill verdict). There is no act row
//   (`item-actions.ts`: "Triage and Grilling are pre-action by definition")
//   and no steps checklist, because there is nothing to act on or break down
//   yet.
// - `"detail"` — the item is minted, so it reads as a record: badges, act row,
//   steps. **Edit** reveals the same fields the triage mode shows, saving
//   through `Core::triage` with `destination: null` (#122's stage-agnostic
//   edit), so this is one more caller of the mutation that already existed,
//   not a new write path.
//
// Everything decidable is still somebody else's: which fields changed is
// `triage-form.ts`, the editing state and #222's clear-on-ok is
// `useItemDraft`, the act vocabulary is `item-actions.ts`, priority's encoding
// is `priority.ts`.

/** One `TaskActionName`'s button label and leading icon — voice per the
 * design README ("Blocked" means an external wait and nothing else, so its
 * label says so rather than leaving the word to speak for itself). */
const ACTION_BUTTON: Record<TaskActionName, { label: string; icon: "play" | "check" | "clock" | "x" }> = {
  start: { label: "Start", icon: "play" },
  complete: { label: "Complete", icon: "check" },
  block: { label: "Mark blocked", icon: "clock" },
  cancel: { label: "Cancel", icon: "x" },
};

/** SKILL.md's grain scale, as the select renders it. */
const GRAINS = [
  { value: "1", label: "Coarse" },
  { value: "2", label: "Default grain" },
  { value: "3", label: "Fine" },
];

export interface ItemPanelProps {
  item: TaskItemDTO;
  /** The Routes the Project select offers. `[]` is a legitimate value — a
   * device with no projects yet — and renders "No project" alone. */
  projects: ProjectDTO[];
  mode: "detail" | "triage";
  /** Whatever `TaskState.stepsByItem[item.id]` currently holds — `[]` until
   * the request answers, same "not yet known" shape every other S9 read uses;
   * there is no separate loading flag. Detail mode only. */
  steps?: StepDTO[];
  /** Detail mode's close control. Triage mode has none: the row's own
   * collapsed header is its close control, so a second one inside the panel
   * would be two ways to do one thing. */
  onClose?: () => void;
  /** S11/#109's act affordances. Omitted (no action row rendered) for a
   * caller that only wants the read-only panel. */
  onAct?: (action: TaskActionName) => void;
  /** Set whenever the most recent act request for THIS item resolved to
   * anything other than `"ok"` (`TaskState.lastAct`, matched by the caller on
   * `itemId`) — a failed act used to be recorded in the store and rendered
   * nowhere, silently. */
  actError?: string | null;
  /** S13/#111's triage mutation, which is also detail mode's Edit save
   * (`destination: null`). Absent for a render with no worker behind it (demo
   * mode), in which case detail mode shows no Edit button at all rather than
   * an editor that could not send anything. */
  onTriage?: (
    itemId: string,
    destination: "ready" | null,
    edits: TriageEdits,
  ) => void;
  /** The most recent triage result any editor got back
   * (`TaskState.lastTriage`) — what clears the typing on an `"ok"` naming this
   * item (#222), and what detail mode renders as a failure. */
  lastTriage?: TaskTriageResult | null;
  /** "Grill me" (#355, ADR-0023): opens the centre-column takeover over this
   * item, decided by `item-actions.ts`'s `canGrill`. Triage mode. */
  onGrillMe?: (itemId: string) => void;
  /** The DOM id the triage row's header points `aria-controls` at. */
  id?: string;
  /** The id a triage row's own "Grill me" button carries, so the screen can
   * re-focus it after the takeover unmounts the list. */
  grillMeId?: string;
  /** #273's microtask affordance. `undefined` in demo mode, which is what
   * guarantees a demo detail view cannot issue a real request. */
  microtask?: {
    run: SkillRunState;
    onRun: (request: MicrotaskRunRequest) => void;
    /** #274's pinned-decline affordance: set only while `run.phase` is
     * `"declined"` AND the current selection is a pin (never Auto) AND the
     * registry has another entry to offer. One tap both switches and retries,
     * but as a **single call** the caller owns: this panel hands over the
     * request it built the first time and nothing else. */
    declinedFallback?: {
      label: string;
      onSwitchAndRun: (request: MicrotaskRunRequest) => void;
    } | null;
  };
}

export function ItemPanel({
  item,
  projects,
  mode,
  steps = [],
  onClose,
  onAct,
  actError = null,
  onTriage,
  lastTriage = null,
  onGrillMe,
  id,
  grillMeId,
  microtask,
}: ItemPanelProps) {
  // Detail mode's Edit state. Always false in triage mode, where the fields
  // are the panel rather than a mode of it.
  const [editing, setEditing] = useState(false);
  // The write landing is what leaves Edit — never the click that sent it. A
  // failed save keeps both the typing and the open editor, which is the whole
  // of #222 applied one surface over.
  const { draft, problems, blocked, set, reset } = useItemDraft(item, lastTriage, () =>
    setEditing(false),
  );

  // Local, and reset per item by the `key` the caller puts on this element: a
  // grain chosen for one item says nothing about the next.
  const [grain, setGrain] = useState("2");
  const run = microtask?.run ?? IDLE;
  const affordance = microtaskAffordance(steps);
  const running = isRunning(run);
  const stamp = stampLabel(run);
  // Which backend answers is an app-level preference (#274's picker,
  // Settings) — never chosen here, and never varied per item or per skill.
  const runRequest: MicrotaskRunRequest =
    affordance.kind === "break"
      ? { itemId: item.id }
      : { itemId: item.id, replace: true, grain: Number(grain) };

  const showFields = mode === "triage" || editing;
  // Detail mode only. On Triage the row renders this itself, *outside* its
  // expanded block, so a late failure still lands on a collapsed row — see
  // `TriageRow`. Rendering it here as well would say it twice.
  const triageError = mode === "detail" ? triageFailureFor(lastTriage, item.id) : null;

  const fields = (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
      <Input
        label="Title"
        size="sm"
        value={draft.title}
        error={problems.title}
        onChange={(event) => set("title", event.target.value)}
      />
      <Textarea
        label="Description"
        rows={3}
        value={draft.description}
        placeholder="The only free-prose field — never a checklist"
        onChange={(event) => set("description", event.target.value)}
      />
      <div
        style={{
          display: "grid",
          // The inner `min()` matters once the container is itself under
          // 160px: `auto-fit` cannot drop below one track, and that one track
          // would otherwise hold its 160px and overflow.
          gridTemplateColumns: "repeat(auto-fit, minmax(min(160px, 100%), 1fr))",
          gap: "var(--space-4)",
          alignItems: "start",
        }}
      >
        <Select
          label="Project"
          size="sm"
          value={draft.projectId}
          onChange={(event) => set("projectId", event.target.value)}
          options={[
            { value: "", label: "No project" },
            ...projects.map((project) => ({ value: project.id, label: project.name })),
          ]}
        />
        <Select
          label="Priority"
          size="sm"
          value={draft.priority}
          onChange={(event) => set("priority", event.target.value)}
          options={PRIORITY_OPTIONS}
        />
        {/* The glyph sits beside the field's *label*, showing the draft's
            current level, and not inside the option list: a native `<option>`
            renders text only — an SVG in one is dropped, and the whole point
            of the family is that the level is visible before the menu is
            opened. An untouched draft ("" — not set) gets the ghost variant,
            which is the resting state, not a prompt to choose.

            (#446/#447 wrote this on `TriageRow`'s editor, which this component
            had already absorbed on this branch; ported rather than lost.) */}
        <Select
          label={
            <>
              <Icon
                name={sizeIcon(draft.size || null)}
                size={13}
                style={{ color: levelColor(draft.size || null) }}
              />
              Size
            </>
          }
          size="sm"
          value={draft.size}
          onChange={(event) => set("size", event.target.value)}
          options={SIZE_OPTIONS}
        />
        <Select
          label={
            <>
              <Icon
                name={energyIcon(draft.energy || null)}
                size={13}
                style={{ color: levelColor(draft.energy || null) }}
              />
              Energy
            </>
          }
          size="sm"
          value={draft.energy}
          onChange={(event) => set("energy", event.target.value)}
          options={ENERGY_OPTIONS}
        />
        <Combobox
          label="Context"
          size="sm"
          value={draft.context}
          onChange={(context) => set("context", context)}
          suggestions={CONTEXTS}
          placeholder="Not set"
        />
        <DeadlineField
          value={draft.deadline}
          error={problems.deadline}
          onChange={(deadline) => set("deadline", deadline)}
        />
        <Input
          label="Scheduled date"
          size="sm"
          type="date"
          value={draft.scheduledDate}
          error={problems.scheduledDate}
          onChange={(event) => set("scheduledDate", event.target.value)}
        />
      </div>
    </div>
  );

  if (mode === "triage") {
    return (
      <div
        id={id}
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "var(--space-5)",
          padding: "var(--space-5)",
          paddingTop: "var(--space-4)",
          borderTop: "1px solid var(--border-subtle)",
        }}
      >
        {fields}
        {/* The source is shown, never edited: provenance belongs to whatever
            captured the item (`TriageEdits`' own doc). */}
        <span className="hb-meta">
          {item.sourceUrl
            ? `source ${item.source ?? "unknown"} · ${item.sourceUrl}`
            : `source ${item.source ?? "typed here"}`}
        </span>
        <div style={{ display: "flex", gap: "var(--space-4)", justifyContent: "flex-end" }}>
          {onGrillMe && canGrill(item.stage) ? (
            <Button
              id={grillMeId}
              size="sm"
              variant="secondary"
              iconLeft="sparkles"
              disabled={item.pending}
              onClick={() => onGrillMe(item.id)}
            >
              Grill me
            </Button>
          ) : null}
          <Button
            size="sm"
            variant="primary"
            iconLeft="check"
            disabled={item.pending || blocked}
            onClick={() => onTriage?.(item.id, "ready", buildTriageEdits(draft, item))}
          >
            Promote to ready
          </Button>
        </div>
      </div>
    );
  }

  return (
    <Card
      elevation={2}
      padding="var(--space-5)"
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
          <span className="hb-meta">{item.seq !== null ? `HB-${item.seq}` : "item detail"}</span>
          {/* The title is the one field that reads as the heading, so while
              editing it is in the grid below rather than repeated here. */}
          <h2 style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>{item.title}</h2>
        </div>
        <div style={{ display: "flex", gap: "var(--space-3)" }}>
          {/* No `onTriage`, no Edit: an editor that cannot send is worse than
              no editor, and demo mode has no worker behind it. No icon on it
              either — the design system's named vocabulary has no pencil, and
              one word needs no glyph to be unambiguous. */}
          {onTriage && !editing ? (
            <Button size="sm" variant="ghost" onClick={() => setEditing(true)}>
              Edit
            </Button>
          ) : null}
          {onClose ? <IconButton icon="x" label="Close item detail" onClick={onClose} /> : null}
        </div>
      </div>

      {showFields ? (
        <>
          {fields}
          <div style={{ display: "flex", gap: "var(--space-4)", justifyContent: "flex-end" }}>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                reset();
                setEditing(false);
              }}
            >
              Discard
            </Button>
            <Button
              size="sm"
              variant="primary"
              iconLeft="check"
              // `destination: null` (#122): this edits the fields and leaves
              // the stage exactly where it is. An item in detail is already
              // past triage, and there is no destination for "where it
              // already was".
              disabled={item.pending || blocked || !hasTriageEdits(draft, item)}
              onClick={() => onTriage?.(item.id, null, buildTriageEdits(draft, item))}
            >
              Save
            </Button>
          </div>
        </>
      ) : (
        <div style={{ display: "flex", gap: "var(--space-3)", flexWrap: "wrap" }}>
          <StageBadge stage={item.stage} />
          {hasPriority(item.priority) ? (
            <Badge tone="brand">{priorityLabel(item.priority)}</Badge>
          ) : null}
          {/* Both always render, unlike the row's meta chips and unlike
              `context` beside them. This is the one surface that describes a
              single item in full, so "nobody has judged this yet" is a fact
              worth showing — as the ghost glyph and an em dash, which is what
              the unset variants exist for. It is a resting state, never a
              warning: muted, same weight as the rest, nothing escalated.

              (#446/#447 wrote this on `ItemDetailPanel`, which this component
              had already replaced on this branch; it is ported here rather
              than lost to the merge.) */}
          <Badge mono icon={sizeIcon(item.size)} style={{ color: levelColor(item.size) }}>
            size:{sizeLabel(item.size)}
          </Badge>
          <Badge mono icon={energyIcon(item.energy)} style={{ color: levelColor(item.energy) }}>
            energy:{energyLabel(item.energy)}
          </Badge>
          {item.context ? <Badge mono>{item.context}</Badge> : null}
        </div>
      )}

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

      {/* `role="alert"` on every one of this app's danger-text paragraphs:
          each renders only once a write has already failed, so it appears with
          no other change on the page and a screen reader would otherwise never
          announce it. Colour was the whole signal.

          Two paragraphs, because an act and an edit are two results the store
          holds at once and either can be the one that failed. */}
      {([actError, triageError] as const).map((message, index) =>
        message ? (
          <p
            key={index}
            role="alert"
            style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)" }}
          >
            {message}
          </p>
        ) : null,
      )}

      {/* While editing, the description is a field above rather than prose. */}
      {!showFields && item.description ? (
        <p style={{ font: "var(--type-body)", color: "var(--text-secondary)" }}>
          {item.description}
        </p>
      ) : null}

      {/* The microtask affordance belongs to the steps block, not the act row
          above: that row is `availableActions(item.stage)` — the funnel — and
          asking for a checklist moves the item through nothing. */}
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
                narration arrives line by line while the user is watching, so
                it must not interrupt. */}
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

            {/* Verbatim, unprefixed, unbranched — #307 made the seam's decline
                prose-only with no reason code precisely so nothing
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
