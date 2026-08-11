import { useEffect, useRef, useState } from "react";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import { Input } from "../components/forms/Input";
import { Select } from "../components/forms/Select";
import { Slider } from "../components/forms/Slider";
import type { DemoCapture, DemoData } from "../fixtures/demo";
import { CAPTURE_INPUT_ID } from "../shell/capture-hotkey";
import type { TriageDestinationName } from "../store/protocol";
import type { TaskState } from "../store/store";
import type { CaptureFields, TriageEdits } from "../store/worker-client";
import { EMPTY_CAPTURE_META, resolveCaptureFields } from "./capture-meta";
import { canSubmitCapture } from "./capture-validation";
import { buildTriageEdits, EMPTY_TRIAGE_DRAFT, type TriageDraft } from "./triage-form";
import { orderTriage } from "./triage-order";
import { SingleColumn } from "./layout";

const CONTEXTS = ["@home", "@computer", "@phone", "@errands", "@garden", "@waiting"];

/** The capture box's Energy/Size `Slider` stops, left to right — the display
 * labels, which are NOT always the domain vocabulary ("normal" is the middle
 * size stop; `hummingbird_domain::Size` calls it `short`).
 *
 * Exported only so `capture-meta.test.ts` can pin them against that module's
 * own `CAPTURE_SIZE_NAMES`/`CAPTURE_ENERGY_NAMES`, which are indexed by the
 * raw slider index and hand-aligned with these. Nothing mechanical connects
 * the two sides: a fourth stop added here and not there resolves to
 * `undefined`, which reads downstream as "not set" — a dropped selection
 * with no error anywhere. The length assertion is that missing mechanism. */
export const CAPTURE_ENERGY_STOPS: string[] = ["low", "medium", "high"];
export const CAPTURE_SIZE_STOPS: string[] = ["quick", "normal", "deep"];

/** S13/#111's triage promotion form: `hummingbird_domain::Size`/`Energy`'s
 * own vocabulary names, exactly (`quick`/`short`/`deep`,
 * `low`/`medium`/`high`) — never the capture box's own looser display
 * labels above, so what a triage sends is resolved by the same name the
 * server parses, never a positional index. */
const TRIAGE_SIZES: Array<{ value: TriageDraft["size"]; label: string }> = [
  { value: "", label: "Not set" },
  { value: "quick", label: "Quick" },
  { value: "short", label: "Short" },
  { value: "deep", label: "Deep" },
];
const TRIAGE_ENERGIES: Array<{ value: TriageDraft["energy"]; label: string }> = [
  { value: "", label: "Not set" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
];

export interface TriageScreenProps {
  demo: DemoData | null;
  /** S12's real triage inbox (issue #110), rendered whenever `demo` is
   * null. */
  task: TaskState;
  /** Enqueues a real capture — `shell/useCaptureWiring.ts`'s `submitCapture`.
   * Never called with an empty/whitespace-only draft: `canSubmitCapture`
   * gates the button this component renders before this is ever reached
   * (#110's "an empty capture is refused client-side"). `fields` (#208)
   * carries the capture box's Energy/Size/Context selections, already
   * resolved to the wire's vocabulary names by `capture-meta.ts`'s
   * `resolveCaptureFields` — never the slider's own indices or the
   * select's raw empty-string resting value. */
  onSubmitCapture: (title: string, nowMs: number, fields: CaptureFields) => void;
  /** S13/#111's triage mutation — `shell/useTriageWiring.ts`'s `triage`.
   * Edits whatever `edits` sets and promotes the item to `destination`, as
   * one call. Optional so a demo-only render (no worker behind it) never
   * has to pass a real one. */
  onTriage?: (itemId: string, destination: TriageDestinationName, edits: TriageEdits) => void;
  /** Bumped by `App.tsx`'s global capture hotkey (and its header Capture
   * button) to request focus on the capture input from anywhere in the
   * app — #110's "a global hotkey that focuses it". `0` (the initial
   * value) never focuses; only a later, distinct value does, so mounting
   * this screen by navigating a screen at a time never steals focus on its
   * own. */
  focusRequestId: number;
}

export function TriageScreen({ demo, task, onSubmitCapture, onTriage, focusRequestId }: TriageScreenProps) {
  const [queue, setQueue] = useState<DemoCapture[]>(demo?.triage ?? []);
  const [draft, setDraft] = useState("");
  const [meta, setMeta] = useState(EMPTY_CAPTURE_META);
  const [triageDrafts, setTriageDrafts] = useState<Record<string, TriageDraft>>({});
  const mounted = useRef(false);

  function triageDraftFor(itemId: string): TriageDraft {
    return triageDrafts[itemId] ?? EMPTY_TRIAGE_DRAFT;
  }

  function setTriageDraftField(
    itemId: string,
    field: keyof TriageDraft,
    value: string,
  ): void {
    setTriageDrafts((current) => ({
      ...current,
      [itemId]: { ...triageDraftFor(itemId), [field]: value },
    }));
  }

  // Reviewer finding on issue #222 (the capture/triage twin of PR #207's
  // act-failure defect): a triage draft used to clear the instant Promote
  // was clicked, optimistically, so a failed write lost the reader's edits
  // AND said nothing about the failure. It now stays put — `promote` only
  // ever sends the mutation; the draft is cleared below, once and only once
  // `task.lastTriage` actually reports `"ok"` for this item.
  function promote(itemId: string, currentTitle: string, destination: TriageDestinationName): void {
    if (!onTriage) {
      return;
    }
    onTriage(itemId, destination, buildTriageEdits(triageDraftFor(itemId), currentTitle));
  }

  // The React-docs "adjusting state when a prop changes" pattern (same one
  // `NowScreen.tsx`'s `RealFrontier` uses for its optimistic item) — guarded
  // on the result's own `seed` so a broadcast already-observed is never
  // reprocessed. A stale `lastTriage` from a previous item's success cannot
  // wipe THIS item's still-in-flight draft, because it is keyed by the
  // itemId the result actually carries, not by whichever row happens to be
  // open.
  const [processedTriageSeed, setProcessedTriageSeed] = useState<string | null>(null);
  if (task.lastTriage && task.lastTriage.seed !== processedTriageSeed) {
    setProcessedTriageSeed(task.lastTriage.seed);
    if (task.lastTriage.kind === "ok") {
      const okItemId = task.lastTriage.itemId;
      setTriageDrafts((current) => {
        if (!(okItemId in current)) {
          return current;
        }
        const next = { ...current };
        delete next[okItemId];
        return next;
      });
    }
  }

  // Matched by item id, same broadcast-recognition contract
  // `NowScreen.tsx`'s `actError` uses for `lastAct` — a failure belongs to
  // whichever item it names, never to "whichever row is open".
  function triageErrorFor(itemId: string): string | null {
    return task.lastTriage && task.lastTriage.itemId === itemId && task.lastTriage.kind !== "ok"
      ? (task.lastTriage.error ?? "That triage didn't apply.")
      : null;
  }

  // Moves focus to the capture input whenever a focus request arrives
  // (including the one that arrives on first mount if this screen was
  // navigated to BY the hotkey/header button itself — `App.tsx` bumps the
  // id and switches screens in the same gesture, and this screen mounts
  // fresh in that case, so the effect below still fires). `mounted` skips
  // only the truly first render (screen already open, id still its initial
  // 0), the same guard `Header.tsx`'s own focus-on-navigate effect uses.
  useEffect(() => {
    if (!mounted.current) {
      mounted.current = true;
      if (focusRequestId === 0) {
        return;
      }
    }
    document.getElementById(CAPTURE_INPUT_ID)?.focus();
  }, [focusRequestId]);

  const canSubmit = canSubmitCapture(draft);

  // Reviewer finding on issue #222: `TaskState.lastCapture` was written on
  // every `captureResult` and read by nothing, so a failed capture left the
  // reader with no signal at all — the same defect class `actError` above
  // already closed for a failed act. A capture has no pre-existing item to
  // key the error against, so it renders near the capture box itself rather
  // than matched by id; `!demo` keeps it out of the fixture-only demo view,
  // which never issues a real capture and so must never wear a stale one
  // from a previous real session. `kind !== "ok"` overwrites itself on the
  // next capture result, so a stale failure never survives a later success.
  const captureError =
    !demo && task.lastCapture && task.lastCapture.kind !== "ok"
      ? (task.lastCapture.error ?? "That capture didn't go through.")
      : null;

  // The exact rule #222 gave triage above, now applied to capture — the two
  // halves of this one screen had drifted apart on it, and #208 tripled what
  // a failed capture discards (the title PLUS size, energy and context). The
  // draft and the three meta selections survive until a result actually
  // reports `"ok"`; while the write is in flight (no result yet) and after a
  // `"failed"` one, everything the reader typed and chose is still here to
  // retry or amend. Same mechanism as `processedTriageSeed`, deliberately:
  // the render-phase "adjusting state when a prop changes" pattern, guarded
  // on the result's own `seed`, so a broadcast already observed can never
  // clear a draft twice and a replayed/stale seed clears nothing at all.
  // A capture carries no item id, so there is no per-item keying to do —
  // the seed IS the identity.
  const [processedCaptureSeed, setProcessedCaptureSeed] = useState<string | null>(null);
  if (task.lastCapture && task.lastCapture.seed !== processedCaptureSeed) {
    setProcessedCaptureSeed(task.lastCapture.seed);
    if (task.lastCapture.kind === "ok") {
      setDraft("");
      setMeta(EMPTY_CAPTURE_META);
    }
  }

  function submit() {
    if (!canSubmit) {
      return;
    }
    if (demo) {
      // Demo mode has no worker behind it and so no `captureResult` will ever
      // arrive to clear on — the fixture queue IS the acknowledgement.
      setQueue((current) => [
        { id: `CAP-${current.length + 8}`, title: draft, source: "Typed here", age: "just now" },
        ...current,
      ]);
      setDraft("");
      setMeta(EMPTY_CAPTURE_META);
      return;
    }
    onSubmitCapture(draft, Date.now(), resolveCaptureFields(meta));
  }

  function drop(id: string) {
    setQueue((current) => current.filter((capture) => capture.id !== id));
  }

  const realTriage = demo ? [] : orderTriage(task.triageInbox);

  return (
    <SingleColumn>
      <Card
        padding="var(--space-6)"
        style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}
      >
        <div style={{ display: "flex", alignItems: "flex-end", gap: "var(--space-5)", flexWrap: "wrap" }}>
          <Input
            id={CAPTURE_INPUT_ID}
            style={{ flex: 1, minWidth: 260 }}
            label="Capture"
            icon="feather"
            placeholder="What's on your mind?"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              // `isComposing` guards an IME composition commit (e.g. an
              // Enter that confirms a candidate while typing Japanese/
              // Chinese/Korean) from being read as "submit" — that Enter
              // belongs to the composition, not to this form.
              if (event.key === "Enter" && !event.nativeEvent.isComposing) {
                submit();
              }
            }}
          />
          <Button size="md" iconLeft="plus" disabled={!canSubmit} onClick={submit}>
            Add to Triage
          </Button>
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(3, 1fr)",
            gap: "var(--space-7)",
            alignItems: "start",
          }}
        >
          <Slider
            label="Energy"
            options={CAPTURE_ENERGY_STOPS}
            value={meta.energy}
            onChange={(energy) => setMeta({ ...meta, energy })}
          />
          <Slider
            label="Size"
            options={CAPTURE_SIZE_STOPS}
            value={meta.size}
            onChange={(size) => setMeta({ ...meta, size })}
          />
          <Select
            label="Context"
            value={meta.context}
            onChange={(event) => setMeta({ ...meta, context: event.target.value })}
            options={[
              { value: "", label: "Not set" },
              ...CONTEXTS.map((context) => ({ value: context, label: context })),
            ]}
          />
        </div>
        {/* One string, not a ternary: #208 made the capture box's
            Energy/Size/Context genuinely persist onto `CreateItem`, so the
            real arm's old "(not yet stored on a real capture)" suffix went
            from true to false — and it sat on the arm that now DOES store
            them, while demo mode got the clean sentence. With the suffix
            gone the two arms said the same thing, so there is nothing left
            to branch on. `TriageScreen.test.tsx` pins the text the real arm
            renders so it cannot silently rot back. */}
        <span className="hb-meta">
          optional — stage, dates and everything else are decided at mint time
        </span>
        {captureError ? (
          <p role="alert" style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)" }}>{captureError}</p>
        ) : null}
      </Card>

      <div>
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            justifyContent: "space-between",
            marginBottom: "var(--space-4)",
          }}
        >
          <h2 style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>Triage</h2>
          {/* The sweeper is off pending the authority move, and nothing in this
              app drains the queue — only demo mode may claim a cadence. */}
          <span className="hb-meta">
            {demo
              ? `${queue.length} unsorted · swept every 15m`
              : `${realTriage.length} unsorted`}
          </span>
        </div>
        {demo ? (
          queue.length === 0 ? (
            <Card padding="0">
              <EmptyState
                icon="inbox"
                headingLevel={3}
                title="Triage is empty"
                body="Everything captured has been sorted. The sweeper drains again in 15 minutes."
              />
            </Card>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
              {queue.map((capture) => (
                <Card
                  key={capture.id}
                  padding="var(--space-5)"
                  style={{ display: "flex", alignItems: "center", gap: "var(--space-5)", flexWrap: "wrap" }}
                >
                  <StageBadge stage="triage" />
                  {/* Found by the G2 visual gate at 768 (`docs/SURFACES.md`):
                      with a plain `flex: 1, minWidth: 0` and no overflow
                      rule, the meta and the button strip take the whole row
                      and this span is squeezed to a few pixels — its text
                      wrapped one word per line and rendered straight through
                      the meta beside it. The `220px` basis is a floor, not a
                      width: the row wraps the meta and buttons onto their own
                      line before the title is starved, and the ellipsis (the
                      same contract `ItemRow` uses) handles what is still too
                      long after that. */}
                  <span style={{ flex: "1 1 220px", minWidth: 0, font: "var(--type-body)", color: "var(--text-primary)",
                    overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {capture.title}
                  </span>
                  <span className="hb-meta" style={{ flex: "0 0 auto", whiteSpace: "nowrap" }}>
                    {capture.source} · {capture.age}
                  </span>
                  <div
                    style={{
                      display: "flex",
                      gap: "var(--space-3)",
                      flexWrap: "wrap",
                      justifyContent: "flex-end",
                    }}
                  >
                    <Button size="sm" variant="quiet" iconLeft="sparkles" onClick={() => drop(capture.id)}>
                      Mint action
                    </Button>
                    <Button
                      size="sm"
                      variant="secondary"
                      iconLeft="help-circle"
                      onClick={() => drop(capture.id)}
                    >
                      Grill
                    </Button>
                    <Button size="sm" variant="ghost" iconLeft="x" onClick={() => drop(capture.id)}>
                      Drop
                    </Button>
                  </div>
                </Card>
              ))}
            </div>
          )
        ) : realTriage.length === 0 ? (
          <Card padding="0">
            <EmptyState
              icon="inbox"
              headingLevel={3}
              title="Triage is empty"
              body="Nothing captured is waiting to be sorted."
            />
          </Card>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            {realTriage.map((item) => {
              const triageDraft = triageDraftFor(item.id);
              return (
                <Card
                  key={item.id}
                  padding="var(--space-5)"
                  style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: "var(--space-5)", flexWrap: "wrap" }}>
                    <StageBadge stage="triage" />
                    {/* Same wrap-then-ellipsis contract as the demo row above
                        — a real capture's title is whatever someone typed, so
                        it is at least as likely to be long as a fixture's. */}
                    <span
                      style={{ flex: "1 1 220px", minWidth: 0, font: "var(--type-body)", color: "var(--text-primary)",
                        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
                    >
                      {item.title}
                    </span>
                    {item.pending ? (
                      <span
                        title="Not yet confirmed by the server"
                        className="hb-meta"
                        style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", flex: "0 0 auto", whiteSpace: "nowrap" }}
                      >
                        <Icon name="loader-circle" size={13} />
                        Pending
                      </span>
                    ) : null}
                  </div>

                  {onTriage ? (
                    <div
                      style={{
                        display: "flex",
                        flexDirection: "column",
                        gap: "var(--space-4)",
                        paddingTop: "var(--space-4)",
                        borderTop: "1px solid var(--border-subtle)",
                      }}
                    >
                      <div
                        style={{
                          display: "grid",
                          gridTemplateColumns: "repeat(4, 1fr)",
                          gap: "var(--space-4)",
                          alignItems: "end",
                        }}
                      >
                        <Input
                          label="Title"
                          size="sm"
                          value={triageDraft.title}
                          placeholder={item.title}
                          onChange={(event) => setTriageDraftField(item.id, "title", event.target.value)}
                        />
                        <Select
                          label="Project"
                          size="sm"
                          value={triageDraft.projectId}
                          onChange={(event) => setTriageDraftField(item.id, "projectId", event.target.value)}
                          options={[
                            { value: "", label: "No project" },
                            ...task.projects.map((project) => ({ value: project.id, label: project.name })),
                          ]}
                        />
                        <Select
                          label="Size"
                          size="sm"
                          value={triageDraft.size}
                          onChange={(event) =>
                            setTriageDraftField(item.id, "size", event.target.value as TriageDraft["size"])
                          }
                          options={TRIAGE_SIZES}
                        />
                        <Select
                          label="Energy"
                          size="sm"
                          value={triageDraft.energy}
                          onChange={(event) =>
                            setTriageDraftField(item.id, "energy", event.target.value as TriageDraft["energy"])
                          }
                          options={TRIAGE_ENERGIES}
                        />
                      </div>
                      <Select
                        label="Context"
                        size="sm"
                        style={{ maxWidth: 220 }}
                        value={triageDraft.context}
                        onChange={(event) => setTriageDraftField(item.id, "context", event.target.value)}
                        options={[
                          { value: "", label: "Not set" },
                          ...CONTEXTS.map((context) => ({ value: context, label: context })),
                        ]}
                      />
                      <div style={{ display: "flex", gap: "var(--space-4)", justifyContent: "flex-end" }}>
                        <Button
                          size="sm"
                          variant="secondary"
                          iconLeft="help-circle"
                          disabled={item.pending}
                          onClick={() => promote(item.id, item.title, "grilling")}
                        >
                          Send to grilling
                        </Button>
                        <Button
                          size="sm"
                          variant="primary"
                          iconLeft="check"
                          disabled={item.pending}
                          onClick={() => promote(item.id, item.title, "ready")}
                        >
                          Promote to ready
                        </Button>
                      </div>
                      {triageErrorFor(item.id) ? (
                        <p
                          role="alert"
                          style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)" }}
                        >
                          {triageErrorFor(item.id)}
                        </p>
                      ) : null}
                    </div>
                  ) : null}
                </Card>
              );
            })}
          </div>
        )}
      </div>

      <Card
        padding="var(--space-5)"
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-5)",
          background: "var(--surface-quiet)",
        }}
      >
        <Icon name="info" size={16} color="var(--text-muted)" />
        <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
          Captures are created here first, then acked in their source. A capture source is drained; a
          context source never is.
        </span>
      </Card>
    </SingleColumn>
  );
}
