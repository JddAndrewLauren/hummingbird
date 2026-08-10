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
import type { DemoData } from "../fixtures/demo";
import { GOOGLE_CLIENT_ID } from "../shell/useCalendarWiring";
import {
  deadLetterHeading,
  SYNC_STATUS_TONE_LABEL,
  syncStatusLabel,
  syncStatusTone,
} from "../shell/sync-status";
import type { DeadLetterEntryDTO } from "../store/protocol";
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
   * journal, and the mirror download. */
  task: TaskState;
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
                      {SYNC_STATUS_TONE_LABEL[tone]}
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
