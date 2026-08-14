// @vitest-environment jsdom

// The regression suite for #118's bindings editor threading.
//
// The pure module under it is separately tested: `canSubmitBinding` refuses
// a blank draft, a no-op draft and any key this build cannot write;
// `bindingValueLabel` reads the three value states apart. What no node test
// can reach is whether the screen actually CALLS them — whether Save really
// consults `canSubmitBinding`, whether an unwritable row really renders no
// field at all, whether the value that leaves is the trimmed one. That
// thread is what these mount, for exactly the reason `test/component.tsx`
// records: three of the S10-S13 PRs shipped UI state with no reader, and
// typecheck cannot see a missing caller.

import { describe, expect, it, vi } from "vitest";

// The calendar section is gated on a build-time env var
// (`VITE_GOOGLE_CLIENT_ID`), which vitest runs without — so without this the
// picker never renders and #121's locked-row tests below would pass on an
// absent control. The only thing stubbed is that constant.
vi.mock("../shell/useCalendarWiring", () => ({ GOOGLE_CLIENT_ID: "test-client-id" }));

import { SettingsScreen } from "./SettingsScreen";
import { bindingDTO, fireEvent, itemDTO, render, screen, taskState } from "../test/component";
import type { BindingDTO, DeadLetterEntryDTO, LedgerRowDTO } from "../store/protocol";
import type { CalendarState, CoreStatus, TaskState } from "../store/store";

const calendar: CalendarState = {
  connected: false,
  needsReconnect: false,
  selectedCalendarIds: [],
  availableCalendars: [],
  lastPollOutcome: null,
  connectPending: false,
  connectError: null,
  eventReads: {},
};

interface SettingsOptions {
  bindings?: BindingDTO[] | null;
  /** #121: the picker only renders at all once a listing has landed, so a
   * test about the locked Trips row has to supply one. */
  calendar?: Partial<CalendarState>;
  status?: CoreStatus;
  withSetBinding?: boolean;
  task?: Partial<TaskState>;
  /** #172's ADR-0010 diagnostic. `undefined` here means "no handshake yet",
   * which is the pre-handshake `null` the store starts at. */
  coreId?: string | null;
  viewOrdinal?: number | null;
  backendSelection?: string;
}

function renderSettings(options: SettingsOptions = {}) {
  const onSetBinding = vi.fn();
  const onSelectionChange = vi.fn();
  const onBackendSelection = vi.fn();
  const tree = (current: SettingsOptions) => (
    <SettingsScreen
      demo={null}
      status={current.status ?? "ready"}
      apiVersion={1}
      coreId={current.coreId ?? null}
      viewOrdinal={current.viewOrdinal ?? null}
      error={null}
      calendar={{ ...calendar, ...current.calendar }}
      themePreference="system"
      onThemePreference={vi.fn()}
      backendSelection={current.backendSelection ?? "auto"}
      onBackendSelection={onBackendSelection}
      onConnect={vi.fn()}
      onSelectionChange={onSelectionChange}
      onRefresh={vi.fn()}
      taskTokenState="resting"
      taskTokenEnteredAtMs={null}
      onSubmitTaskToken={vi.fn()}
      onForgetTaskToken={vi.fn()}
      task={taskState({ bindings: current.bindings ?? null, ...current.task })}
      onSetBinding={current.withSetBinding === false ? undefined : onSetBinding}
      online
      syncNowMs={10_000}
      onDownloadMirror={vi.fn()}
    />
  );
  const { rerender } = render(tree(options));
  // A pull arriving is a re-render with new props, not a remount — which is
  // the whole point of the stale-draft test below.
  return {
    onSetBinding,
    onSelectionChange,
    onBackendSelection,
    pull: (next: SettingsOptions) => rerender(tree(next)),
  };
}

function saveButton(name: RegExp | string = /save/i): HTMLElement {
  const buttons = screen.getAllByRole("button", { name });
  // "Save token" is the device-token form's own button; the binding rows'
  // are the plain "Save" ones.
  const binding = buttons.filter((button) => button.textContent?.trim() === "Save");
  if (binding.length !== 1) {
    throw new Error(`expected exactly one binding Save button, found ${binding.length}`);
  }
  return binding[0];
}

describe("SettingsScreen — the bindings editor", () => {
  it("says the bindings are unavailable rather than empty while the core is loading", () => {
    renderSettings({ status: "loading" });
    expect(screen.getByText(/bindings are unavailable/i)).toBeDefined();
  });

  it("distinguishes 'no answer yet' from 'nothing is bound'", () => {
    // `bindings: null` is "nobody has answered"; it must not render as a
    // list of unset rows a person could act on.
    renderSettings({ bindings: null });
    expect(screen.getByText(/reading the bindings/i)).toBeDefined();
    expect(screen.queryByLabelText("Race series")).toBeNull();
  });

  it("renders each binding's current value in words, unset ones included", () => {
    renderSettings({
      bindings: [
        bindingDTO({ key: "race-series", value: { state: "text", text: "f1" } }),
        bindingDTO({ key: "trips-calendar", value: { state: "unset" } }),
      ],
    });

    expect(screen.getByText("f1")).toBeDefined();
    expect(screen.getByText("Not set")).toBeDefined();
  });

  it("refuses a blank draft and accepts a real one, sending the trimmed value", () => {
    const { onSetBinding } = renderSettings({
      bindings: [bindingDTO({ key: "race-series", value: { state: "unset" } })],
    });
    const input = screen.getByLabelText("Race series");

    expect(saveButton().hasAttribute("disabled")).toBe(true);

    fireEvent.change(input, { target: { value: "   " } });
    expect(saveButton().hasAttribute("disabled")).toBe(true);

    fireEvent.change(input, { target: { value: "  motogp  " } });
    expect(saveButton().hasAttribute("disabled")).toBe(false);

    fireEvent.click(saveButton());
    expect(onSetBinding).toHaveBeenCalledTimes(1);
    expect(onSetBinding).toHaveBeenCalledWith("race-series", "motogp");
  });

  it("refuses a draft identical to the stored value — a CAS write with no change", () => {
    const { onSetBinding } = renderSettings({
      bindings: [bindingDTO({ key: "race-series", value: { state: "text", text: "f1" } })],
    });

    // The field starts at the stored value, so Save starts disabled.
    expect((screen.getByLabelText("Race series") as HTMLInputElement).value).toBe("f1");
    expect(saveButton().hasAttribute("disabled")).toBe(true);
    expect(onSetBinding).not.toHaveBeenCalled();
  });

  it("marks a queued write and still shows the value it wrote", () => {
    renderSettings({
      bindings: [
        bindingDTO({ key: "race-series", pending: true, value: { state: "text", text: "motogp" } }),
      ],
    });

    expect(screen.getByText("queued")).toBeDefined();
    expect(screen.getByText("motogp")).toBeDefined();
  });

  it("renders a key this build cannot write read-only — no field, no button", () => {
    // `settings` has no DELETE, so a key this build cannot name is one it
    // must not overwrite either.
    renderSettings({
      bindings: [
        bindingDTO({
          key: "some-future-binding",
          known: false,
          value: { state: "other", raw: "7" },
        }),
      ],
    });

    expect(screen.getByText(/not a text value: 7/i)).toBeDefined();
    expect(screen.queryByLabelText("some-future-binding")).toBeNull();
    expect(
      screen.queryAllByRole("button").filter((button) => button.textContent?.trim() === "Save"),
    ).toEqual([]);
  });

  it("reseeds the field when another device's value arrives, rather than sitting on a stale draft", () => {
    // The acceptance criterion this protects: a binding edited on one
    // client is VISIBLE on a second after its next pull. The label always
    // refreshed; the field did not, leaving a stale draft over the new
    // value with Save enabled to push it straight back.
    const { onSetBinding, pull } = renderSettings({
      bindings: [bindingDTO({ key: "race-series", value: { state: "text", text: "f1" } })],
    });
    expect((screen.getByLabelText("Race series") as HTMLInputElement).value).toBe("f1");

    pull({
      bindings: [bindingDTO({ key: "race-series", value: { state: "text", text: "indycar" } })],
    });

    expect((screen.getByLabelText("Race series") as HTMLInputElement).value).toBe("indycar");
    expect(saveButton().hasAttribute("disabled")).toBe(true);
    expect(onSetBinding).not.toHaveBeenCalled();
  });

  it("keeps an in-progress draft while the value underneath it has not moved", () => {
    // Reseeding on every render would fight the typist — only a real change
    // to the value may take the field back.
    const { pull } = renderSettings({
      bindings: [bindingDTO({ key: "race-series", value: { state: "text", text: "f1" } })],
    });
    fireEvent.change(screen.getByLabelText("Race series"), { target: { value: "motog" } });

    pull({
      bindings: [bindingDTO({ key: "race-series", value: { state: "text", text: "f1" } })],
      task: { syncOutcomeSeq: 3 },
    });

    expect((screen.getByLabelText("Race series") as HTMLInputElement).value).toBe("motog");
  });

  it("says so when a binding write failed, on that binding's own row", () => {
    // Without this the outcome was recorded in `lastBindingWrite` and read
    // nowhere: Save appeared to do nothing at all.
    renderSettings({
      bindings: [
        bindingDTO({ key: "race-series" }),
        bindingDTO({ key: "trips-calendar", value: { state: "unset" } }),
      ],
      task: {
        lastBindingWrite: {
          seed: "s",
          key: "race-series",
          kind: "failed",
          error: "the queue could not be written",
        },
      },
    });

    expect(screen.getByText(/the queue could not be written/i)).toBeDefined();
    // And announces it: the danger colour was otherwise the whole signal.
    expect(screen.getByRole("alert").textContent).toBe("the queue could not be written");
  });

  it("offers no Save at all when the host cannot write bindings", () => {
    renderSettings({
      bindings: [bindingDTO({ key: "race-series" })],
      withSetBinding: false,
    });
    const input = screen.getByLabelText("Race series");
    fireEvent.change(input, { target: { value: "motogp" } });

    // A Save that silently does nothing is worse than one that says it
    // cannot — it stays disabled rather than pretending.
    expect(saveButton().hasAttribute("disabled")).toBe(true);
  });
});

// #172: ADR-0010's probe ships as a permanent diagnostic in the "Local
// core" card, because a standalone PWA window has no URL bar and cannot
// reach a throwaway page. This is the gate `src/test/component.tsx` names: `coreId` and
// `viewOrdinal` could be threaded all the way from the handshake and never
// rendered, and typecheck would see nothing wrong.
describe("SettingsScreen — the core-instance diagnostic", () => {
  it("renders the instance id and this view's ordinal once the handshake has landed", () => {
    renderSettings({ coreId: "3f2a1b8c", viewOrdinal: 2 });

    expect(screen.getByText("Core instance 3f2a1b8c · this view #2.")).toBeDefined();
  });

  it("renders no line at all before the handshake", () => {
    renderSettings();

    expect(screen.queryByText(/core instance/i)).toBeNull();
  });

  it("still renders while the core is loading, if a previous handshake supplied it", () => {
    // The line sits outside the card's status ternary deliberately: the
    // build version beside it is known in every state, and so is this once
    // it has arrived at all.
    renderSettings({ status: "loading", coreId: "aa11bb22", viewOrdinal: 1 });

    expect(screen.getByText("Core instance aa11bb22 · this view #1.")).toBeDefined();
  });
});

describe("SettingsScreen — the calendar picker's locked Trips row (#121)", () => {
  const listed = [
    { id: "primary", summary: "john@twinion.net" },
    { id: "trips@g", summary: "Trips" },
  ];
  const boundTrips: BindingDTO[] = [
    bindingDTO({ key: "trips-calendar", value: { state: "text", text: "trips@g" } }),
  ];

  it("renders the bound Trips calendar checked, locked, and with the reason said out loud", () => {
    renderSettings({
      bindings: boundTrips,
      calendar: { availableCalendars: listed, selectedCalendarIds: ["primary"] },
    });

    const trips = screen.getByRole("checkbox", { name: /Trips/ }) as HTMLInputElement;
    expect(trips.checked).toBe(true);
    // Locked, not merely re-ticked: a control that springs back would be a
    // control that lied about what it does.
    expect(trips.disabled).toBe(true);
    expect(screen.getByText(/Polled because it answers/)).toBeDefined();
    // And a route to where the decision actually lives.
    expect(screen.getByRole("link", { name: "Standing questions" })).toBeDefined();
  });

  it("refuses to untick it — the click never reaches the selection handler", () => {
    const { onSelectionChange } = renderSettings({
      bindings: boundTrips,
      calendar: { availableCalendars: listed, selectedCalendarIds: ["primary"] },
    });

    fireEvent.click(screen.getByRole("checkbox", { name: /Trips/ }));
    expect(onSelectionChange).not.toHaveBeenCalled();
  });

  it("still toggles every other calendar, over the polled set the binding contributes to", () => {
    const { onSelectionChange } = renderSettings({
      bindings: boundTrips,
      calendar: { availableCalendars: listed, selectedCalendarIds: ["primary"] },
    });

    fireEvent.click(screen.getByRole("checkbox", { name: /john@twinion.net/ }));
    // The derived id rides along in the request; `acceptSelectionChange`
    // strips it before anything is persisted.
    expect(onSelectionChange).toHaveBeenCalledWith(["trips@g"]);
  });

  it("locks nothing while no Trips calendar is designated", () => {
    renderSettings({
      bindings: [bindingDTO({ key: "trips-calendar", value: { state: "unset" } })],
      calendar: { availableCalendars: listed, selectedCalendarIds: ["primary"] },
    });

    expect((screen.getByRole("checkbox", { name: /Trips/ }) as HTMLInputElement).disabled).toBe(
      false,
    );
    expect(screen.queryByText(/Polled because it answers/)).toBeNull();
  });
});

describe("SettingsScreen — the microtask backend picker (#274)", () => {
  it("lists Auto plus every registered entry, defaulting to Auto", () => {
    renderSettings();
    const select = screen.getByLabelText("Microtask backend") as HTMLSelectElement;
    expect(select.value).toBe("auto");
    const optionLabels = Array.from(select.options).map((option) => option.textContent);
    expect(optionLabels).toEqual(["Auto", "Cloud runner"]);
  });

  it("reflects a pinned selection", () => {
    renderSettings({ backendSelection: "cloud" });
    expect((screen.getByLabelText("Microtask backend") as HTMLSelectElement).value).toBe("cloud");
  });

  it("reports a change without deciding anything itself — the caller owns persistence", () => {
    const { onBackendSelection } = renderSettings();
    fireEvent.change(screen.getByLabelText("Microtask backend"), { target: { value: "cloud" } });
    expect(onBackendSelection).toHaveBeenCalledWith("cloud");
  });
});

describe("SettingsScreen — the dead-letter journal", () => {
  const entry = (overrides: Partial<DeadLetterEntryDTO> = {}): DeadLetterEntryDTO => ({
    id: "q-1",
    reason: "permanent",
    message: "validation",
    fields: [],
    atMs: 5_000,
    entity: "items",
    entityId: "a-1",
    ...overrides,
  });

  const ledgerRow = (id: string, title: string): LedgerRowDTO => ({
    ...itemDTO({ id, title, stage: "ready" }),
    absentSinceMs: null,
    deadLettered: true,
    hasLiveAlert: false,
  });

  // The thread this file exists to mount: the naming rule itself is unit-tested
  // in `dead-letter-subject.test.ts`, and what no node test can see is whether
  // the row actually calls it with the ledger the screen holds.
  it("names the item an abandoned change was about, not just the queue entry", () => {
    renderSettings({
      task: {
        deadLetters: [entry()],
        ledger: [ledgerRow("a-1", "Ring the plumber")],
      },
    });

    expect(screen.getByText('item "Ring the plumber"')).toBeDefined();
    // The queue entry's own id stays on the row — it names the attempt, which
    // is still what a person quotes when asking why it was abandoned.
    expect(screen.getByText("q-1")).toBeDefined();
  });

  it("falls back to the id rather than nothing when the ledger cannot name it", () => {
    renderSettings({ task: { deadLetters: [entry({ entityId: "a-9" })] } });
    expect(screen.getByText("item a-9")).toBeDefined();
  });

  it("says the entity alone when the change named no row", () => {
    renderSettings({
      task: { deadLetters: [entry({ entity: "settings", entityId: null })] },
    });
    expect(screen.getByText("setting")).toBeDefined();
  });
});
