import { useState } from "react";
import type { ReactNode } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { CalendarPicker } from "../components/domain/CalendarPicker";
import { Input } from "../components/forms/Input";
import { Select } from "../components/forms/Select";
import { Switch } from "../components/forms/Switch";
import { toggleCalendarId, unavailableSelectedIds } from "../calendar/selection";
import {
  bindingCopy,
  bindingDraftSeed,
  bindingSubmitValue,
  bindingValueLabel,
  bindingWriteError,
  canSubmitBinding,
  sameBindingValue,
} from "./bindings";
import type { DemoData } from "../fixtures/demo";
import { GOOGLE_CLIENT_ID } from "../shell/useCalendarWiring";
import {
  deadLetterHeading,
  syncStatusLabel,
  syncStatusTone,
  syncStatusToneWord,
} from "../shell/sync-status";
import type { BindingDTO, DeadLetterEntryDTO } from "../store/protocol";
import type { CalendarState, CoreStatus, TaskState } from "../store/store";
import type { TaskTokenSubmitOutcome } from "../task/token";
import { formatEnteredAt, taskQueueStatusCopy, type TaskTokenUiState } from "../task/token-ui";
import type { ThemePreference } from "../theme/theme";
import { Aside, Column, Section, TwoColumn } from "./layout";

const THEME_OPTIONS = [
  { value: "system", label: "Follow system" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

function isThemePreference(value: string): value is ThemePreference {
  return value === "system" || value === "light" || value === "dark";
}

/** A single-sentence card: what is not there, and why. */
function Note({ children }: { children: ReactNode }) {
  return (
    <Card padding="var(--space-6)">
      <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>{children}</p>
    </Card>
  );
}

/** The one place a view can say that the task binding is dead (post-batch
 * review of PR #185). The calendar side is genuinely still ready — #171
 * decoupled the two on purpose — so this is a card inside Settings, not the
 * whole-app `{type:"error"}` screen; but without it every capture, sync and
 * pushed token vanished into a `console.error` while the UI looked healthy.
 * Honesty over reassurance (design README): state what is broken, then the
 * one action that can fix it. The underlying message is kept in the mono
 * meta style — it is machine text, not a sentence. */
function TaskHostUnavailableCard({ message }: { message: string }) {
  return (
    <>
      <span className="hb-meta">tasks</span>
      <Card
        padding="var(--space-5)"
        style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}
      >
        <Badge dot tone="danger" style={{ alignSelf: "flex-start" }}>
          Unavailable
        </Badge>
        <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
          The task core did not start, so nothing captured here is being saved or synced. Reload to
          try again.
        </p>
        <span
          style={{
            font: "var(--type-meta)",
            letterSpacing: "var(--tracking-meta)",
            color: "var(--text-muted)",
            overflowWrap: "anywhere",
          }}
        >
          {message}
        </span>
      </Card>
    </>
  );
}

/** The submit outcomes the form itself needs to say something about — every
 * `TaskTokenSubmitOutcome` except `"ok"`, which clears the field instead of
 * showing an error. */
type TokenFormError = Exclude<TaskTokenSubmitOutcome, "ok">;

const TOKEN_FORM_ERROR_COPY: Record<TokenFormError, string> = {
  blank: "Enter a token before saving.",
  storeError: "Could not save the token on this device. Try again.",
};

/** The device-token entry field, shared between the first-run "unset" state
 * and the 401 "reprompt" state — same form, different surrounding copy.
 * `type="password"` keeps the value out of the on-screen render (shoulder
 * surfing) and Chrome's enhanced-spellcheck exfiltration path;
 * `autoComplete="off"` keeps a long-lived bearer token out of the browser's
 * saved-password store, which is scoped to this origin and outlives
 * "forget token"; `spellCheck={false}` is the same exfiltration surface
 * `autoComplete` addresses, belt-and-braces since some browsers spellcheck
 * password fields anyway. The field clears on a successful submit so a
 * stale value never lingers on screen once it is safely in IndexedDB. */
function TokenEntryForm({
  onSubmit,
}: {
  onSubmit: (input: string) => Promise<TaskTokenSubmitOutcome>;
}) {
  const [value, setValue] = useState("");
  const [error, setError] = useState<TokenFormError | null>(null);

  async function submit() {
    const outcome = await onSubmit(value);
    if (outcome === "ok") {
      setValue("");
      setError(null);
      return;
    }
    setError(outcome);
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
      <Input
        label="Device token"
        placeholder="hb_device_..."
        type="password"
        autoComplete="off"
        spellCheck={false}
        value={value}
        error={error === null ? undefined : TOKEN_FORM_ERROR_COPY[error]}
        onChange={(event) => {
          setValue(event.target.value);
          setError(null);
        }}
      />
      <Button onClick={() => void submit()} fullWidth>
        Save token
      </Button>
    </div>
  );
}

/** One binding row (#118): what it is for, what it currently holds, and a
 * field to change it. The current value is stated in words above the field
 * rather than only pre-filled into it, because "Not set" and an empty field
 * read identically once typing starts — and the value the pane will actually
 * use is the stored one until a cycle drains this write.
 *
 * A row for a key this build cannot write renders the value and stops: no
 * field, no button. `settings` has no DELETE, so a key this build cannot
 * name is one it must not overwrite either. */
function BindingRow({
  binding,
  writeError,
  onSetBinding,
}: {
  binding: BindingDTO;
  /** The last `setBinding` failure for THIS key, already matched and worded
   * (`bindingWriteError`) — `null` when the last write succeeded or was some
   * other row's. */
  writeError: string | null;
  onSetBinding?: (key: string, value: string) => void;
}) {
  const copy = bindingCopy(binding);
  const [draft, setDraft] = useState(() => bindingDraftSeed(binding.value));

  // Reseed the field whenever the value underneath it moves — a pull
  // carrying another device's edit, or this device's own write confirming.
  // The row is keyed by binding key and so never remounts, which means the
  // mount-time seed alone would leave a stale draft sitting over a value it
  // never showed, with Save enabled to push it back (#118 review finding).
  // React's own "adjust state while rendering" idiom: cheaper than an effect
  // and applied before anything paints.
  const [seenValue, setSeenValue] = useState(binding.value);
  if (!sameBindingValue(seenValue, binding.value)) {
    setSeenValue(binding.value);
    setDraft(bindingDraftSeed(binding.value));
  }

  const canSubmit = canSubmitBinding(binding, draft) && onSetBinding !== undefined;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: "var(--space-4)" }}>
        <span className="hb-meta">{binding.key}</span>
        {binding.pending ? (
          <Badge dot mono tone="warn">
            queued
          </Badge>
        ) : null}
      </div>
      <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
        {bindingValueLabel(binding.value)}
      </p>
      {binding.known ? (
        // The help line sits under the whole row rather than inside the
        // field's own `hint`: with a hint, the field's box grows downward and
        // the Save button — bottom-aligned to the tallest item — drifts below
        // the input it belongs to, worst at the 768px wrap point.
        <>
          <div style={{ display: "flex", alignItems: "flex-end", gap: "var(--space-4)" }}>
            <Input
              label={copy.label}
              value={draft}
              style={{ flex: 1, minWidth: 0 }}
              onChange={(event) => setDraft(event.target.value)}
            />
            <Button
              variant="secondary"
              disabled={!canSubmit}
              onClick={() => {
                if (!canSubmit) {
                  return;
                }
                onSetBinding?.(binding.key, bindingSubmitValue(draft));
              }}
            >
              Save
            </Button>
          </div>
          {writeError ? (
            <p style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)" }}>
              {writeError}
            </p>
          ) : null}
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>{copy.help}</p>
        </>
      ) : (
        <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>{copy.help}</p>
      )}
    </div>
  );
}

/** One dead-lettered entry's field-level detail — S9's "1 edit didn't
 * apply" affordance. No dedicated Table component exists in the kit (16
 * components, none of them tabular), so this renders as a bordered list of
 * rows in the mono meta style the rest of the app already uses for computed
 * values, the same idiom `TriageScreen.tsx`'s capture rows use. */
function DeadLetterRow({ entry }: { entry: DeadLetterEntryDTO }) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-2)",
        padding: "var(--space-4) 0",
        borderTop: "1px solid var(--border-subtle)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <span className="hb-meta">{entry.id}</span>
        <span className="hb-meta">{new Date(entry.atMs).toISOString()}</span>
      </div>
      {entry.reason === "permanent" ? (
        <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
          {entry.message ?? "Rejected — no further detail."}
        </p>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
          {entry.fields.map((field) => (
            <div
              key={field.field}
              style={{
                display: "grid",
                gridTemplateColumns: "auto 1fr 1fr",
                gap: "var(--space-4)",
                alignItems: "baseline",
              }}
            >
              <span className="hb-meta">{field.field}</span>
              <span style={{ font: "var(--type-body-sm)", color: "var(--text-primary)" }}>
                local: {JSON.stringify(field.local)}
              </span>
              <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
                server: {JSON.stringify(field.server)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export interface SettingsScreenProps {
  demo: DemoData | null;
  status: CoreStatus;
  apiVersion: number | null;
  error: string | null;
  calendar: CalendarState;
  themePreference: ThemePreference;
  onThemePreference: (preference: ThemePreference) => void;
  onConnect: () => void;
  onSelectionChange: (selectedCalendarIds: string[]) => void;
  onRefresh: () => void;
  /** #106/S8's device-token surface — entry, rest, and re-prompt. */
  taskTokenState: TaskTokenUiState;
  taskTokenEnteredAtMs: number | null;
  onSubmitTaskToken: (input: string) => Promise<TaskTokenSubmitOutcome>;
  onForgetTaskToken: () => void;
  /** S9's sync-status affordance: last sweep, queue depth, the dead-letter
   * journal, and the mirror download — and #118's bindings, read from
   * `task.bindings`. */
  task: TaskState;
  /** #118's binding write. Absent (a demo render, a core that never came
   * up) renders every binding read-only rather than a Save button that
   * silently does nothing. */
  onSetBinding?: (key: string, value: string) => void;
  online: boolean;
  syncNowMs: number;
  onDownloadMirror: () => void;
}

export function SettingsScreen({
  demo,
  status,
  apiVersion,
  error,
  calendar,
  themePreference,
  onThemePreference,
  onConnect,
  onSelectionChange,
  onRefresh,
  taskTokenState,
  taskTokenEnteredAtMs,
  onSubmitTaskToken,
  onForgetTaskToken,
  task,
  onSetBinding,
  online,
  syncNowMs,
  onDownloadMirror,
}: SettingsScreenProps) {
  const unavailableIds = unavailableSelectedIds(
    calendar.selectedCalendarIds,
    calendar.availableCalendars,
  );

  // Demo mode toggles a local copy and nothing else: the fixture ids are not
  // real calendars, and routing them through `onSelectionChange` would persist
  // them to localStorage and poll the worker for calendars that do not exist.
  const [demoSelectedIds, setDemoSelectedIds] = useState<string[]>(() => {
    const first = demo?.calendars[0]?.id;
    return first === undefined ? [] : [first];
  });

  // The design-system picker always renders its fieldset, so an empty one is
  // an empty box captioned "Calendars to poll" — a control over nothing.
  const hasCalendars = calendar.availableCalendars.length > 0 || unavailableIds.length > 0;

  return (
    <TwoColumn>
      <Column>
        <Section title="Calendar context">
          {!GOOGLE_CLIENT_ID ? (
            <Note>Calendar context is unavailable: this build has no Google client id.</Note>
          ) : status !== "ready" ? (
            <Note>Calendar context is unavailable until the local core loads.</Note>
          ) : demo || hasCalendars ? (
            <CalendarPicker
              calendars={demo ? demo.calendars : calendar.availableCalendars}
              selectedIds={demo ? demoSelectedIds : calendar.selectedCalendarIds}
              unavailableIds={demo ? [] : unavailableIds}
              onToggle={(id) =>
                demo
                  ? setDemoSelectedIds((current) => toggleCalendarId(current, id))
                  : onSelectionChange(toggleCalendarId(calendar.selectedCalendarIds, id))
              }
            />
          ) : (
            <Note>
              No calendars have been listed yet — nothing to choose from until Google Calendar
              returns a list.
            </Note>
          )}
        </Section>

        <Section title="Standing questions">
          {demo === null && status !== "ready" ? (
            <Note>Bindings are unavailable until the local core loads.</Note>
          ) : demo === null && task.bindings === null ? (
            <Note>Reading the bindings.</Note>
          ) : (
            <Card
              padding="var(--space-6)"
              style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}
            >
              <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
                What each standing question is about. These are workspace facts — a change here
                reaches every device on its next sync.
              </p>
              {(demo === null ? (task.bindings ?? []) : demo.bindings).map((binding) => (
                <BindingRow
                  key={binding.key}
                  binding={binding}
                  // A demo row can never have been written, so it can never
                  // have failed to write.
                  writeError={
                    demo === null ? bindingWriteError(task.lastBindingWrite, binding.key) : null
                  }
                  // Demo rows are fixtures, not real `settings` rows: a Save
                  // here would enqueue a CAS write for a key nobody bound.
                  onSetBinding={demo === null ? onSetBinding : undefined}
                />
              ))}
            </Card>
          )}
        </Section>

        <Section title="This device">
          <Card
            padding="var(--space-6)"
            style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}
          >
            <Select
              label="Theme"
              value={themePreference}
              options={THEME_OPTIONS}
              onChange={(event) => {
                if (isThemePreference(event.target.value)) {
                  onThemePreference(event.target.value);
                }
              }}
            />
            {demo ? <Switch label="Show acked alerts" /> : null}
          </Card>
        </Section>

        {demo ? (
          <Section title="Mirror">
            <Card
              padding="var(--space-6)"
              style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}
            >
              <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
                Derived and disposable. Deleting it loses nothing.
              </p>
              <Button variant="secondary" iconLeft="rotate-ccw" style={{ alignSelf: "flex-start" }}>
                Rebuild
              </Button>
            </Card>
          </Section>
        ) : null}
      </Column>

      <Aside label="Core and calendar status">
        <span className="hb-meta">core</span>
        <Card
          padding="var(--space-5)"
          style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
        >
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
            <span style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>
              Local core
            </span>
            <Badge
              mono
              tone={status === "ready" ? "success" : status === "error" ? "danger" : "neutral"}
            >
              {status}
            </Badge>
          </div>
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            {status === "ready"
              ? `Worker and wasm loaded${apiVersion === null ? "" : ` · api v${apiVersion}`}.`
              : status === "error"
                ? (error ?? "The core failed to load.")
                : "Loading the worker and wasm core."}
          </p>
        </Card>

        {status === "ready" && GOOGLE_CLIENT_ID ? (
          <>
            <span className="hb-meta">google calendar</span>
            <Card
              padding="var(--space-5)"
              style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
            >
              {calendar.connected && calendar.needsReconnect ? (
                <p style={{ font: "var(--type-body-sm)", color: "var(--status-warn-fg)" }}>
                  The credential no longer works. The last snapshot is still showing, and stays
                  honest about its age.
                </p>
              ) : null}
              {!calendar.connected ? (
                <Button iconLeft="calendar-clock" onClick={onConnect} fullWidth>
                  Connect Google Calendar
                </Button>
              ) : calendar.needsReconnect ? (
                <Button iconLeft="calendar-clock" onClick={onConnect} fullWidth>
                  Reconnect Google Calendar
                </Button>
              ) : (
                <Button
                  variant="secondary"
                  iconLeft="refresh-cw"
                  onClick={onRefresh}
                  fullWidth
                  data-testid="refresh-calendar"
                >
                  Refresh calendar
                </Button>
              )}
              <span className="hb-meta">
                opt-in is per-device · polled every 15m in the foreground
              </span>
            </Card>
          </>
        ) : null}

        {status === "ready" ? (
          <>
            <span className="hb-meta">device token</span>
            <Card
              padding="var(--space-5)"
              style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
            >
              <p
                style={{
                  font: "var(--type-body-sm)",
                  color:
                    taskTokenState === "reprompt"
                      ? "var(--status-warn-fg)"
                      : "var(--text-secondary)",
                }}
              >
                {taskQueueStatusCopy(taskTokenState)}
              </p>
              {taskTokenState === "resting" ? (
                <>
                  {taskTokenEnteredAtMs !== null ? (
                    <span className="hb-meta">entered {formatEnteredAt(taskTokenEnteredAtMs)}</span>
                  ) : null}
                  <Button
                    variant="secondary"
                    iconLeft="x"
                    onClick={onForgetTaskToken}
                    style={{ alignSelf: "flex-start" }}
                  >
                    Forget token
                  </Button>
                </>
              ) : (
                <TokenEntryForm onSubmit={onSubmitTaskToken} />
              )}
            </Card>
          </>
        ) : null}

        {task.hostError !== null ? <TaskHostUnavailableCard message={task.hostError} /> : null}

        {status === "ready" ? (
          <>
            <span className="hb-meta">sync</span>
            <Card
              padding="var(--space-5)"
              style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
            >
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                <span style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>
                  Outbound queue
                </span>
                <Badge mono>{task.queueDepth ?? 0} queued</Badge>
              </div>
              {(() => {
                const syncStatusInput = {
                  online,
                  lastSyncOutcome: task.lastSyncOutcome,
                  lastSyncAtMs: task.lastSyncAtMs,
                  queueDepth: task.queueDepth,
                  nowMs: syncNowMs,
                };
                const tone = syncStatusTone(syncStatusInput);
                return (
                  <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
                    <Badge dot mono tone={tone} style={{ alignSelf: "flex-start" }}>
                      {syncStatusToneWord(syncStatusInput)}
                    </Badge>
                    <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
                      {syncStatusLabel(syncStatusInput)}
                    </p>
                  </div>
                );
              })()}
              <Button
                variant="secondary"
                iconLeft="download"
                onClick={onDownloadMirror}
                style={{ alignSelf: "flex-start" }}
              >
                Download mirror
              </Button>
            </Card>

            {task.deadLetters.length > 0 ? (
              <>
                <span className="hb-meta">{deadLetterHeading(task.deadLetters.length)}</span>
                <Card padding="var(--space-5)">
                  {task.deadLetters.map((entry) => (
                    <DeadLetterRow key={`${entry.id}-${entry.atMs}`} entry={entry} />
                  ))}
                </Card>
              </>
            ) : null}
          </>
        ) : null}
      </Aside>
    </TwoColumn>
  );
}
