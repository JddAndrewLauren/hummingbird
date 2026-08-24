import { useState } from "react";
import type { ReactNode } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { CalendarPicker } from "../components/domain/CalendarPicker";
import { Icon } from "../components/core/Icon";
import { Input } from "../components/forms/Input";
import { Select } from "../components/forms/Select";
import { Switch } from "../components/forms/Switch";
import { connectErrorCopy } from "../calendar/connect-error";
import { toggleCalendarId, unavailableSelectedIds } from "../calendar/selection";
import { AUTO_SELECTION, BACKEND_REGISTRY } from "../skills/backend-registry";
import {
  bindingCopy,
  bindingDraftSeed,
  bindingSubmitValue,
  bindingValueLabel,
  bindingWriteError,
  canSubmitBinding,
  groupBindingsByQuestion,
  questionLabelForBinding,
  questionSwitchWriteError,
  sameBindingValue,
} from "./bindings";
import { ControlButton, SECTION_TOGGLE_HOVER, sectionToggleStyle } from "./ControlButton";
import {
  readExpandedQuestions,
  toggleExpandedQuestion,
  writeExpandedQuestions,
} from "./question-prefs";
import { questionRoster } from "./questions/roster";
import { APP_VERSION } from "../shell/build-version";
import { coreInstanceLabel } from "../shell/status-label";
import {
  TRIPS_CALENDAR_BINDING_KEY,
  effectiveCalendarIds,
  tripsCalendarId,
} from "../calendar/selection";
import {
  deadLetterHeading,
  syncStatusLabel,
  syncStatusTone,
  syncStatusToneWord,
} from "../shell/sync-status";
import type { BindingDTO, DeadLetterEntryDTO, LedgerRowDTO } from "../store/protocol";
import type { StorageLike } from "./storage";
import type { CalendarState, CoreStatus, TaskState } from "../store/store";
import type { TaskTokenSubmitOutcome } from "../task/token";
import { formatEnteredAt, taskQueueStatusCopy, type TaskTokenUiState } from "../task/token-ui";
import type { ThemePreference } from "../theme/theme";
import { deadLetterSubject } from "./dead-letter-subject";
import { Aside, Column, Section, TwoColumn } from "./layout";

const THEME_OPTIONS = [
  { value: "system", label: "Follow system" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

// #274's picker: Auto first (the sensible default, never a hidden fallback)
// then every registered entry, in the same order Auto itself walks. This
// slice registers only the cloud runner — #275/#276 lengthen the list by
// registering theirs, never by changing this mapping.
const BACKEND_OPTIONS = [
  { value: AUTO_SELECTION, label: "Auto" },
  ...BACKEND_REGISTRY.map((entry) => ({ value: entry.id, label: entry.label })),
];

/** The leftovers group's own key in the device-local open-rows set
 * (`question-prefs.ts`). Not a question, so it can never collide with one:
 * the vocabulary is closed and every member of it is kebab-case with no
 * dot. */
export const OTHER_ROWS_KEY = "other.settings-rows";

function isThemePreference(value: string): value is ThemePreference {
  return value === "system" || value === "light" || value === "dark";
}

/** What the last Connect/Reconnect press did, when it failed. Two lines: what
 * happened, then what to do about it — an error the reader cannot act on is
 * just bad news. Both come from `calendar/connect-error.ts`; this places them
 * and nothing else. */
function ConnectError({ error }: { error: string }) {
  const { message, hint } = connectErrorCopy(error);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
      <p style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)" }}>{message}</p>
      <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>{hint}</p>
    </div>
  );
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
            <p role="alert" style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)" }}>
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

/** One question's group in the `Standing questions` section (#714, made a
 * disclosure with a toggle at #715): the question in the reader's own words,
 * where it renders, whether it is switched on, and — once opened — its
 * toggle and the binding rows that answer it.
 *
 * **Collapsed by default, and the collapse state is device-local**
 * (`question-prefs.ts`). Ten questions each with a value line, a field and a
 * Save button is a screenful nobody reads; and `bindings.rs` is explicit
 * that a collapse state is never a binding, which matters more here than
 * anywhere else in the app because the row it belongs to is the one whose
 * *toggle* does sync.
 *
 * **A group with no rows still renders, and still opens.** The roster is the
 * one place a question can be seen when its own pane is quiet — ADR-0034
 * decision 4 makes that load-bearing — so "nothing to set here" is a fact
 * worth stating, and a question with no binding is still switchable.
 *
 * The heading stays an `h3` and keeps its own accessible name: the
 * disclosure button lives *inside* it (`FrontierColumns.tsx`'s own idiom, by
 * way of `sectionToggleStyle`), so the section's heading structure is
 * unchanged and nothing new competes with the question's name. */
function QuestionGroup({
  heading,
  meta,
  note,
  rows,
  missing,
  enabled,
  pending,
  switchError,
  expanded,
  onToggleExpanded,
  lastBindingWrite,
  onSetBinding,
  onSetEnabled,
}: {
  heading: string;
  /** The mono meta word beside the heading — the surface this question
   * renders on, or what the leftover group is. */
  meta: string;
  /** Said when the question has nothing to configure at all. */
  note: string;
  rows: BindingDTO[];
  /** Keys this question declares that the reader was handed no row for —
   * a different fact from having none, and said as one. */
  missing: string[];
  /** `null` for a group that is not a question (the leftovers) and for one
   * whose switch this reader has not been handed — no toggle is drawn
   * either way, rather than one guessed at. */
  enabled: boolean | null;
  pending: boolean;
  /** The last `setQuestionEnabled` failure for THIS question, already
   * matched and worded (`questionSwitchWriteError`). */
  switchError: string | null;
  expanded: boolean;
  onToggleExpanded: () => void;
  lastBindingWrite: TaskState["lastBindingWrite"];
  onSetBinding?: (key: string, value: string) => void;
  /** Absent (a core that never came up) draws the toggle read-only rather
   * than one that silently does nothing — `onSetBinding`'s own contract. */
  onSetEnabled?: (enabled: boolean) => void;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      {/* Wraps rather than squeezing: at 390px a question carrying both
          state badges left the heading two words wide. The badges taking
          their own line is the honest trade — the question's name is what
          the row is for. */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-4)",
          flexWrap: "wrap",
        }}
      >
        {/* h3 under the section's h2 — `--type-body-strong` is the size
            token, not the level. The button is inside the heading so the
            heading keeps the question's name as its own accessible name. */}
        <h3 style={{ margin: 0, flex: "1 1 14rem", minWidth: 0, font: "inherit" }}>
          <ControlButton
            aria-expanded={expanded}
            onClick={onToggleExpanded}
            baseStyle={sectionToggleStyle(!expanded)}
            hoverStyle={SECTION_TOGGLE_HOVER}
          >
            <Icon
              name="chevron-down"
              size={14}
              style={{
                color: "var(--text-muted)",
                transform: expanded ? "none" : "rotate(-90deg)",
                transition: "transform var(--dur-fast) var(--ease-flit)",
              }}
            />
            <span style={{ flex: 1, minWidth: 0 }}>{heading}</span>
          </ControlButton>
        </h3>
        {/* Both state words stay readable while the row is shut. A question
            switched off is discoverable only here (ADR-0034's consequences),
            so "off" must never be a fact you have to expand a row to find. */}
        {enabled === false ? (
          <Badge dot mono tone="neutral">
            off
          </Badge>
        ) : null}
        {pending ? (
          <Badge dot mono tone="warn">
            queued
          </Badge>
        ) : null}
        <span className="hb-meta">{meta}</span>
      </div>
      {expanded ? (
        // Indented behind a hairline rather than nested in a second Card:
        // the rows belong to the question above them, and the design system
        // caps a region at two card elevations.
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: "var(--space-6)",
            paddingLeft: "var(--space-5)",
            borderLeft: "1px solid var(--border-subtle)",
          }}
        >
          {enabled === null ? null : (
            <>
              <Switch
                aria-label={`Asked — ${heading}`}
                label="Asked"
                hint="Off hides this question's panes, silences its alerts and stops it being polled."
                checked={enabled}
                onChange={
                  onSetEnabled === undefined
                    ? undefined
                    : (event) => onSetEnabled(event.target.checked)
                }
              />
              {switchError ? (
                <p
                  role="alert"
                  style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)" }}
                >
                  {switchError}
                </p>
              ) : null}
            </>
          )}
          {rows.length === 0 && missing.length === 0 ? (
            <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>{note}</p>
          ) : null}
          {rows.map((binding) => (
            <BindingRow
              key={binding.key}
              binding={binding}
              writeError={bindingWriteError(lastBindingWrite, binding.key)}
              onSetBinding={onSetBinding}
            />
          ))}
          {missing.map((key) => (
            <p key={key} style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
              No settings row for <span className="hb-meta">{key}</span> yet.
            </p>
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** One dead-lettered entry's field-level detail — S9's "1 edit didn't
 * apply" affordance. No dedicated Table component exists in the kit (16
 * components, none of them tabular), so this renders as a bordered list of
 * rows in the mono meta style the rest of the app already uses for computed
 * values, the same idiom `TriageScreen.tsx`'s capture rows use. */
function DeadLetterRow({
  entry,
  ledger,
}: {
  entry: DeadLetterEntryDTO;
  ledger: readonly LedgerRowDTO[] | null;
}) {
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
        {/* What the abandoned change was ABOUT, in body type — the queue
            entry's own id stays in the mono meta style beside it, because it
            is machine material and this is not. Until the entry carried its
            subject, this row led with that id and a person had no way to tell
            which of their edits had been given up on. */}
        <span style={{ font: "var(--type-body-sm)", color: "var(--text-primary)" }}>
          {deadLetterSubject(entry, ledger)}
        </span>
        <span className="hb-meta">{new Date(entry.atMs).toISOString()}</span>
      </div>
      <span className="hb-meta">{entry.id}</span>
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
                // `minmax(0, 1fr)`, not a bare `1fr`: a `1fr` track is
                // min-content-floored, and these two hold `JSON.stringify`
                // output that has no wrap opportunity — one long value pushed
                // the row wider than the page rather than wrapping in place.
                gridTemplateColumns: "auto minmax(0, 1fr) minmax(0, 1fr)",
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
  status: CoreStatus;
  apiVersion: number | null;
  /** #172's ADR-0010 diagnostic — see `coreInstanceLabel`. Both `null`
   * before the handshake, which renders no line at all. */
  coreId: string | null;
  viewOrdinal: number | null;
  error: string | null;
  calendar: CalendarState;
  /** True only when `calendar` is the board world's fixture (#452, piece 4),
   * never for a live `CalendarState` — the calendar card needs its own
   * signal to bypass the device-token/`status` gates below, which describe
   * the live wiring's preconditions and mean nothing for a fixture that was
   * never fetched through it. It also routes the card's toggles to the
   * local demo copy — through `onSelectionChange` they would persist
   * fixture ids to the real device selection and poll Google for calendars
   * that do not exist. */
  calendarIsDemo: boolean;
  themePreference: ThemePreference;
  onThemePreference: (preference: ThemePreference) => void;
  /** #274's picker choice — `AUTO_SELECTION` or a registered entry's id,
   * device-local and never synced (`useBackendSelection.ts`). */
  backendSelection: string;
  onBackendSelection: (selection: string) => void;
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
  /** #118's binding write. Absent (a core that never came up) renders every
   * binding read-only rather than a Save button that silently does
   * nothing. */
  onSetBinding?: (key: string, value: string) => void;
  /** #715's toggle write. Absent (a core that never came up) draws every
   * question's switch read-only, `onSetBinding`'s own contract. */
  onSetQuestionEnabled?: (question: string, enabled: boolean) => void;
  /** Injected storage for this screen's one device-local view preference —
   * which question rows are open (`question-prefs.ts`). Defaults to
   * `localStorage` when the environment has one, so a caller that passes
   * nothing (every test that mounts this screen without the prop) behaves
   * exactly as before. */
  storage?: StorageLike;
  online: boolean;
  syncNowMs: number;
  onDownloadMirror: () => void;
  /** #707's SharedWorker diagnostic journal — "Download diagnostics" and
   * "Clear diagnostics", beside the mirror download above. A separate
   * export from the mirror's own: the journal and the mirror are two
   * different files, on purpose (see `shell/diagnostics-download.ts`). */
  onDownloadDiagnostics: () => void;
  onClearDiagnostics: () => void;
}

export function SettingsScreen({
  status,
  apiVersion,
  coreId,
  viewOrdinal,
  error,
  calendar,
  calendarIsDemo,
  themePreference,
  onThemePreference,
  backendSelection,
  onBackendSelection,
  onConnect,
  onSelectionChange,
  onRefresh,
  taskTokenState,
  taskTokenEnteredAtMs,
  onSubmitTaskToken,
  onForgetTaskToken,
  task,
  onSetBinding,
  onSetQuestionEnabled,
  storage,
  online,
  syncNowMs,
  onDownloadMirror,
  onDownloadDiagnostics,
  onClearDiagnostics,
}: SettingsScreenProps) {
  const instanceLabel = coreInstanceLabel(coreId, viewOrdinal);
  const unavailableIds = unavailableSelectedIds(
    calendar.selectedCalendarIds,
    calendar.availableCalendars,
  );
  // The board world's fixture calendar toggles a local copy and nothing
  // else: the fixture ids are not real calendars, and routing them through
  // `onSelectionChange` would persist them to localStorage and poll the
  // worker for calendars that do not exist.
  const [demoSelectedIds, setDemoSelectedIds] = useState<string[]>(() => {
    const first = calendarIsDemo ? calendar.availableCalendars[0]?.id : undefined;
    return first === undefined ? [] : [first];
  });

  // #121: designating a Trips calendar opts this device into polling it, so
  // its row renders checked and locked with the reason said out loud —
  // never a calendar fetched with nothing on screen to explain it. The
  // fixture world has no bindings table to read or real calendars to poll,
  // so it locks nothing.
  const tripsId = calendarIsDemo ? null : tripsCalendarId(task.bindings);

  // #714: the roster is the core's, so both the section below and the
  // picker's locked-row hint name a question the same way. Falls back to the
  // raw key only if nothing claims it — a state the core's own test rules
  // out, said honestly rather than guessed at.
  const roster = questionRoster();
  const groupedBindings = groupBindingsByQuestion(
    roster,
    task.bindings ?? [],
    task.questionSwitches,
  );

  // #715: which question rows are open, device-local. Resolved once, the
  // same way `NowScreen` resolves its own — a caller that passes nothing
  // gets `localStorage` where there is one, and a session-only preference
  // where there is not.
  const resolvedStorage =
    storage ?? (typeof localStorage === "undefined" ? undefined : localStorage);
  const [expandedQuestions, setExpandedQuestions] = useState<ReadonlySet<string>>(() =>
    readExpandedQuestions(resolvedStorage),
  );
  const toggleQuestionRow = (question: string) => {
    const next = toggleExpandedQuestion(expandedQuestions, question);
    setExpandedQuestions(next);
    writeExpandedQuestions(resolvedStorage, next);
  };
  const tripsQuestionLabel =
    questionLabelForBinding(roster, TRIPS_CALENDAR_BINDING_KEY) ?? TRIPS_CALENDAR_BINDING_KEY;
  const polledIds = calendarIsDemo
    ? demoSelectedIds
    : effectiveCalendarIds(calendar.selectedCalendarIds, tripsId);

  // The design-system picker always renders its fieldset, so an empty one is
  // an empty box captioned "Calendars to poll" — a control over nothing.
  const hasCalendars = calendar.availableCalendars.length > 0 || unavailableIds.length > 0;

  return (
    <TwoColumn>
      <Column>
        <Section title="Calendar context">
          {!calendarIsDemo && taskTokenState === "unset" ? (
            <Note>
              Calendar context is unavailable: this device has no device token, and a token is
              what authorises polling. Enter one under "device token" to opt this device in.
            </Note>
          ) : !calendarIsDemo && status !== "ready" ? (
            <Note>Calendar context is unavailable until the local core loads.</Note>
          ) : calendarIsDemo || hasCalendars ? (
            <>
              <CalendarPicker
                calendars={calendar.availableCalendars}
                selectedIds={polledIds}
                unavailableIds={calendarIsDemo ? [] : unavailableIds}
                lockedIds={tripsId === null ? [] : [tripsId]}
                lockedHint={
                  <>
                    Polled because it answers <em>{tripsQuestionLabel}</em>. Change it under{" "}
                    <a href="#standing-questions">Standing questions</a>.
                  </>
                }
                onToggle={(id) =>
                  calendarIsDemo
                    ? setDemoSelectedIds((current) => toggleCalendarId(current, id))
                    : onSelectionChange(toggleCalendarId(polledIds, id))
                }
              />
            </>
          ) : (
            <Note>
              No calendars have been listed yet — nothing to choose from until Google Calendar
              returns a list.
            </Note>
          )}
        </Section>

        <Section title="Standing questions" id="standing-questions">
          {status !== "ready" ? (
            <Note>Bindings are unavailable until the local core loads.</Note>
          ) : task.bindings === null ? (
            <Note>Reading the bindings.</Note>
          ) : (
            <Card
              padding="var(--space-6)"
              style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}
            >
              <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
                Every standing question this build asks, and what each one is set to. These are
                workspace facts — a change here reaches every device on its next sync.
              </p>
              {groupedBindings.groups.map((group) => (
                <QuestionGroup
                  key={group.question}
                  heading={group.label}
                  meta={group.surface}
                  note="Nothing to set — this question reads no source anyone chose."
                  rows={group.rows}
                  missing={group.missing}
                  enabled={group.enabled}
                  pending={group.pending}
                  switchError={questionSwitchWriteError(
                    task.lastQuestionSwitchWrite,
                    group.question,
                  )}
                  expanded={expandedQuestions.has(group.question)}
                  onToggleExpanded={() => toggleQuestionRow(group.question)}
                  lastBindingWrite={task.lastBindingWrite}
                  onSetBinding={onSetBinding}
                  onSetEnabled={
                    onSetQuestionEnabled === undefined
                      ? undefined
                      : (enabled) => onSetQuestionEnabled(group.question, enabled)
                  }
                />
              ))}
              {/* Rows in the table that no question claims — in practice the
                  keys this build cannot write. `Core::bindings` returns them
                  on purpose (`bindings.rs`), so dropping them here would
                  hide what is really stored. */}
              {groupedBindings.other.length === 0 ? null : (
                <QuestionGroup
                  heading="Other settings rows"
                  meta="unclaimed"
                  note=""
                  rows={groupedBindings.other}
                  missing={[]}
                  // Not a question: nothing switches it on or off, and it
                  // is nobody's `lastQuestionSwitchWrite`.
                  enabled={null}
                  pending={false}
                  switchError={null}
                  expanded={expandedQuestions.has(OTHER_ROWS_KEY)}
                  onToggleExpanded={() => toggleQuestionRow(OTHER_ROWS_KEY)}
                  lastBindingWrite={task.lastBindingWrite}
                  onSetBinding={onSetBinding}
                />
              )}
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
            {/* #274: which backend a microtask run prefers. Auto is the
                default and is itself a choice, not a fallback hidden behind
                one — picking a concrete entry pins the run to it. */}
            <Select
              label="Microtask backend"
              value={backendSelection}
              options={BACKEND_OPTIONS}
              onChange={(event) => onBackendSelection(event.target.value)}
            />
          </Card>
        </Section>
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
          {/* The build version, in the one place there is room for the
              unabbreviated form — and outside the status ternary above,
              since it is known in every one of those three states. */}
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            {`Build v${APP_VERSION}.`}
          </p>
          {/* #172: ADR-0010's probe, and the reason it ships rather than
              being a throwaway page — a standalone PWA window has no URL
              bar, so the only reachable place to read this is inside the
              app's own `start_url`. Same instance id in two windows means
              they share one core. Absent until the handshake lands. */}
          {instanceLabel !== null ? (
            <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
              {instanceLabel}
            </p>
          ) : null}
        </Card>

        {/* #707's SharedWorker diagnostic journal. Review round 1 of PR
            #736: this used to sit inside the `status === "ready"` gate
            below with the mirror download, which meant the journal was
            unexportable exactly when the core never reaches ready — one of
            the main situations an operator needs it (a hang or a wasm
            load failure during startup, #704's own incident). The journal
            lives in the SharedWorker's own module scope independent of
            whether wasm ever loads (`worker/diagnostics-journal.ts`), and
            `worker/ports.ts`'s `DiagnosticsPortHandler` now answers
            `getDiagnostics`/`clearDiagnostics` even from a core that failed
            to initialize (see that module's own doc) — so these two
            controls render unconditionally, never gated on `status`. The
            "Download mirror" button above stays `status === "ready"`-gated
            correctly: the mirror is a wasm-side read with nothing to serve
            it from a core that never loaded. */}
        <span className="hb-meta">diagnostics</span>
        <Card
          padding="var(--space-5)"
          style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
        >
          {status !== "ready" ? (
            <Note>
              {status === "loading"
                ? "The local core is still loading — a request queues and completes once it does."
                : "The local core failed to start, but the SharedWorker's own request/response journal is a separate, independent store and is still reachable."}
            </Note>
          ) : null}
          <div style={{ display: "flex", gap: "var(--space-3)", flexWrap: "wrap" }}>
            <Button
              variant="secondary"
              iconLeft="download"
              onClick={onDownloadDiagnostics}
              style={{ alignSelf: "flex-start" }}
            >
              Download diagnostics
            </Button>
            <Button
              variant="secondary"
              iconLeft="trash-2"
              onClick={onClearDiagnostics}
              style={{ alignSelf: "flex-start" }}
            >
              Clear diagnostics
            </Button>
          </div>
        </Card>

        {status === "ready" && taskTokenState !== "unset" ? (
          <>
            <span className="hb-meta">google calendar</span>
            <Card
              padding="var(--space-5)"
              style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
            >
              {/* Two readings of the same reconnect state. The blocked one
                  replaces the ordinary sentence rather than joining it: the
                  app has stopped retrying, and saying only "the credential no
                  longer works" would leave the reader waiting for a recovery
                  that is never coming. `calendar/remint-health.ts` decides.
                  #581: this sentence names no cause. It is one string for all
                  five of `remint-health.ts`'s `BLOCKING_ERRORS`, which have
                  five different causes — the earlier "most likely a revoked
                  refresh token" was right for one of them and sent the reader
                  hunting a revocation that never happened for the other four.
                  The cause is `connect-error.ts`'s per-code copy, rendered
                  directly beneath; this says only the part that copy does not,
                  which is that retrying has stopped. */}
              {calendar.connected && calendar.needsReconnect ? (
                <p style={{ font: "var(--type-body-sm)", color: "var(--status-warn-fg)" }}>
                  {calendar.silentRemintBlocked
                    ? "The credential no longer works, and renewing it in the background has stopped working too. The last snapshot is still showing, and stays honest about its age. Retry polling on this device once it's fixed."
                    : "The credential no longer works. The last snapshot is still showing, and stays honest about its age."}
                </p>
              ) : null}
              {/* What the last attempt did, in words. Before this the button
                  simply did nothing on every failure path — no pending state
                  and no error, ever. The copy is `calendar/connect-error.ts`;
                  this only places it. */}
              {calendar.connectError !== null ? (
                <ConnectError error={calendar.connectError} />
              ) : null}
              {/* #585: there is no sign-in any more — consent happened once
                  in the operator's terminal (ADR-0028) — so the button reads
                  as what it does, opting this one device into polling,
                  never as a login. */}
              {!calendar.connected ? (
                <Button
                  iconLeft="calendar-clock"
                  onClick={onConnect}
                  fullWidth
                  loading={calendar.connectPending}
                  disabled={calendar.connectPending}
                >
                  Poll Google Calendar on this device
                </Button>
              ) : calendar.needsReconnect ? (
                <Button
                  iconLeft="calendar-clock"
                  onClick={onConnect}
                  fullWidth
                  loading={calendar.connectPending}
                  disabled={calendar.connectPending}
                >
                  Retry polling on this device
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
                    <DeadLetterRow
                      key={`${entry.id}-${entry.atMs}`}
                      entry={entry}
                      ledger={task.ledger}
                    />
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
