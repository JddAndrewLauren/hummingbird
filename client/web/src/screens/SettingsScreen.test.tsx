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

import { OTHER_ROWS_KEY, SettingsScreen } from "./SettingsScreen";
import { connectErrorCopy } from "../calendar/connect-error";
import { questionRoster } from "./questions/roster";
import { bindingDTO, fireEvent, itemDTO, render, screen, taskState } from "../test/component";
import type { BindingDTO, DeadLetterEntryDTO, LedgerRowDTO } from "../store/protocol";
import type { CalendarState, CoreStatus, TaskState } from "../store/store";
import type { TaskTokenUiState } from "../task/token-ui";

// Every code `calendar/authority-token-client.ts` can produce (that
// module's own header lists and explains them) — kept here rather than
// exported from `connect-error.ts` itself, since duplicating the list is
// what lets a test iterate it independently of the switch it is checking.
const ALL_CONNECT_ERROR_CODES = [
  "no_device_token",
  "authority_rejected_device_token",
  "authority_unconfigured",
  "authority_upstream",
  "authority_unreachable",
  "bad_token_response",
  "no_access_token",
] as const;

const calendar: CalendarState = {
  connected: false,
  needsReconnect: false,
  selectedCalendarIds: [],
  availableCalendars: [],
  lastPollOutcome: null,
  connectPending: false,
  connectError: null,
  silentRemintBlocked: false,
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
  /** #585: the calendar gates key off this, not a build-time env var — most
   * cases in this file want a token present, so the default is "resting"
   * and only the device-token-precondition tests below override it. */
  taskTokenState?: TaskTokenUiState;
  /** #715: the roster's rows are collapsed by default and their open state
   * is device-local, so a test about a binding row has to open the question
   * first. Rather than click through ten disclosures in every one of them,
   * this seeds the injected storage with every row already open — the state
   * these tests were all written in. The default itself is not thereby
   * untested: the roster block below asserts it directly, both directions. */
  expanded?: readonly string[];
}

/** A `StorageLike` over a plain object — never the real `localStorage`,
 * which leaks the open-rows preference between tests (and, per this repo's
 * own notes, is not reliably present in every node the suite runs on). */
function stubStorage(seed: Record<string, string> = {}) {
  const store = new Map(Object.entries(seed));
  return {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
    read: (key: string) => store.get(key) ?? null,
  };
}

function renderSettings(options: SettingsOptions = {}) {
  const onConnect = vi.fn();
  const onSetBinding = vi.fn();
  const onSetQuestionEnabled = vi.fn();
  const storage = stubStorage({
    "hb.settings.questions-expanded": JSON.stringify(
      options.expanded ?? [...questionRoster().map((entry) => entry.question), OTHER_ROWS_KEY],
    ),
  });
  const onSelectionChange = vi.fn();
  const onBackendSelection = vi.fn();
  const onDownloadDiagnostics = vi.fn();
  const onClearDiagnostics = vi.fn();
  const tree = (current: SettingsOptions) => (
    <SettingsScreen
      status={current.status ?? "ready"}
      apiVersion={1}
      coreId={current.coreId ?? null}
      viewOrdinal={current.viewOrdinal ?? null}
      error={null}
      calendar={{ ...calendar, ...current.calendar }}
      calendarIsDemo={false}
      themePreference="system"
      onThemePreference={vi.fn()}
      backendSelection={current.backendSelection ?? "auto"}
      onBackendSelection={onBackendSelection}
      onConnect={onConnect}
      onSelectionChange={onSelectionChange}
      onRefresh={vi.fn()}
      taskTokenState={current.taskTokenState ?? "resting"}
      taskTokenEnteredAtMs={null}
      onSubmitTaskToken={vi.fn()}
      onForgetTaskToken={vi.fn()}
      task={taskState({ bindings: current.bindings ?? null, ...current.task })}
      onSetBinding={current.withSetBinding === false ? undefined : onSetBinding}
      onSetQuestionEnabled={
        current.withSetBinding === false ? undefined : onSetQuestionEnabled
      }
      storage={storage}
      online
      syncNowMs={10_000}
      onDownloadMirror={vi.fn()}
      onDownloadDiagnostics={onDownloadDiagnostics}
      onClearDiagnostics={onClearDiagnostics}
    />
  );
  const { rerender, unmount } = render(tree(options));
  // A pull arriving is a re-render with new props, not a remount — which is
  // the whole point of the stale-draft test below.
  return {
    onConnect,
    onSetBinding,
    onSetQuestionEnabled,
    onSelectionChange,
    onBackendSelection,
    onDownloadDiagnostics,
    onClearDiagnostics,
    storage,
    /** Tears this mount down, so a test can mount a SECOND screen over the
     * same injected storage — which is what a reload is, and the only way
     * to prove a device-local preference survives one. */
    unmount,
    pull: (next: SettingsOptions) => rerender(tree(next)),
  };
}

/** The text of the nearest `h3` before `node` in document order — the
 * question a row is nested under. Read from the rendered DOM rather than
 * from a test id, because "the row is under the right heading" is exactly
 * what a reader sees and a `data-` attribute would let drift. */
function nearestQuestionHeading(node: HTMLElement): string | null {
  const headings = Array.from(document.querySelectorAll("h3"));
  let found: string | null = null;
  for (const heading of headings) {
    if (heading.compareDocumentPosition(node) & Node.DOCUMENT_POSITION_FOLLOWING) {
      found = heading.textContent;
    }
  }
  return found;
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
describe("SettingsScreen — the standing-question roster (#714, ADR-0034)", () => {
  it("lists every standing question the core knows, in the core's order", () => {
    // The section's spine is the core's roster, not the bindings table: a
    // fresh device with nothing set still sees all eleven questions.
    renderSettings({ bindings: [] });

    const headings = screen
      .getAllByRole("heading", { level: 3 })
      .map((heading) => heading.textContent);
    for (const entry of questionRoster()) {
      expect(headings).toContain(entry.label);
    }
  });

  it("nests each binding under the question it answers", () => {
    renderSettings({
      bindings: [
        bindingDTO({ key: "race-series", value: { state: "text", text: "f1" } }),
        bindingDTO({ key: "trips-calendar", value: { state: "text", text: "trips@g" } }),
      ],
    });

    // The row's own key is drawn inside the question's group, so the
    // question heading has to be the nearest preceding h3.
    for (const [key, question] of [
      ["race-series", "race"],
      ["trips-calendar", "vacation"],
    ] as const) {
      const row = screen.getByText(key).closest("div");
      expect(row).not.toBeNull();
      expect(nearestQuestionHeading(row as HTMLElement)).toBe(
        questionRoster().find((entry) => entry.question === question)?.label,
      );
    }
  });

  it("renders a question with no bindings with an empty body rather than omitting it", () => {
    renderSettings({ bindings: [] });

    const weekend = questionRoster().find((entry) => entry.question === "weekend");
    expect(weekend?.bindings).toEqual([]);
    expect(
      screen.getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent),
    ).toContain(weekend?.label);
    expect(screen.getAllByText(/nothing to set/i).length).toBeGreaterThan(0);
  });

  it("says a declared binding has no row rather than saying there is nothing to set", () => {
    // The state the demo world actually renders: `scps` declares
    // `scps-quest`, and the seeded bindings list does not carry it.
    renderSettings({ bindings: [bindingDTO({ key: "race-series" })] });

    const scps = questionRoster().find((entry) => entry.question === "scps");
    expect(scps?.bindings).toEqual(["scps-quest"]);
    // Four of the five keys are absent from this list, so the line is
    // located by its own key rather than by the sentence around it.
    const said = screen.getByText("scps-quest").closest("p") as HTMLElement;
    expect(said.textContent).toBe("No settings row for scps-quest yet.");
    expect(nearestQuestionHeading(said)).toBe(scps?.label);
    // And it is not the "nothing to set" line, which would be the opposite
    // claim about the same question.
    expect(said.textContent).not.toMatch(/nothing to set/i);
  });

  it("keeps a row no question claims, under 'Other settings rows', still read-only", () => {
    renderSettings({
      bindings: [
        bindingDTO({ key: "race-series", value: { state: "text", text: "f1" } }),
        bindingDTO({
          key: "some-future-binding",
          known: false,
          value: { state: "other", raw: "7" },
        }),
      ],
    });

    const row = screen.getByText("some-future-binding").closest("div") as HTMLElement;
    expect(nearestQuestionHeading(row)).toBe("Other settings rows");
    expect(screen.getByText(/not a text value: 7/i)).toBeDefined();
    expect(screen.queryByLabelText("some-future-binding")).toBeNull();
  });

  it("draws no leftovers group when every live row belongs to a question", () => {
    renderSettings({ bindings: [bindingDTO({ key: "race-series" })] });
    expect(screen.queryByText("Other settings rows")).toBeNull();
  });

  it("still renders the whole roster with no device token — the section is not token-gated", () => {
    // The calendar section above it is (#585); this one is not, and a
    // device with no token must still be able to read what is bound.
    renderSettings({ bindings: [], taskTokenState: "unset" });

    const headings = screen
      .getAllByRole("heading", { level: 3 })
      .map((heading) => heading.textContent);
    expect(headings).toContain(questionRoster()[0].label);
  });

  it("draws no question at all while the core is still loading", () => {
    // The `status !== "ready"` guard is above the roster, so a loading core
    // gets the one honest note and not a roster of empty groups that looks
    // like an answer.
    renderSettings({ status: "loading", bindings: [] });

    expect(screen.getByText(/bindings are unavailable/i)).toBeDefined();
    // By level, not by text: the `reachability` question is called "This
    // device", which is also the title of the section below the roster —
    // an h2. The roster's own headings are the h3s, and there are none.
    expect(screen.queryAllByRole("heading", { level: 3 })).toEqual([]);
  });
});

describe("SettingsScreen — the off switch (#715, ADR-0034)", () => {
  /** The roster row for one question, by its label. */
  function row(question: string): HTMLElement {
    const label = questionRoster().find((entry) => entry.question === question)?.label;
    if (label === undefined) {
      throw new Error(`no roster entry for ${question}`);
    }
    return screen.getByRole("button", { name: label });
  }

  /** Whether one question's row is open, read off the disclosure's own
   * `aria-expanded` — the same fact a screen reader is told, not a class. */
  function isOpen(question: string): boolean {
    return row(question).getAttribute("aria-expanded") === "true";
  }

  function allEnabled() {
    return questionRoster().map((entry) => ({
      question: entry.question,
      enabled: true,
      pending: false,
    }));
  }

  it("draws every question's row shut, with no toggle and no binding row on show", () => {
    // The default, asserted rather than inherited from the harness: ten
    // questions each with a value line, a field and a Save button is a
    // screenful nobody reads.
    renderSettings({
      expanded: [],
      bindings: [bindingDTO({ key: "race-series", value: { state: "text", text: "f1" } })],
      task: { questionSwitches: allEnabled() },
    });

    for (const entry of questionRoster()) {
      expect(isOpen(entry.question)).toBe(false);
    }
    expect(screen.queryAllByRole("switch")).toEqual([]);
    expect(screen.queryByText("race-series")).toBeNull();
  });

  it("reveals the question's toggle and its bindings when the row is opened", () => {
    // The gesture the issue names: "expanding a question's row reveals its
    // bindings and its toggle".
    const { storage } = renderSettings({
      expanded: [],
      bindings: [bindingDTO({ key: "race-series", value: { state: "text", text: "f1" } })],
      task: { questionSwitches: allEnabled() },
    });

    fireEvent.click(row("race"));

    expect(isOpen("race")).toBe(true);
    expect(screen.getByText("race-series")).toBeDefined();
    const toggle = screen.getByRole("switch", { name: /race/i });
    expect((toggle as HTMLInputElement).checked).toBe(true);
    // Only the opened row's toggle: the other nine are still shut.
    expect(screen.getAllByRole("switch")).toHaveLength(1);
    // And the gesture persisted device-locally, in the injectable storage —
    // never in `settings`, which has no DELETE and syncs everywhere.
    expect(JSON.parse(storage.read("hb.settings.questions-expanded") ?? "null")).toEqual(["race"]);
  });

  it("keeps a row open across a reload, reading it back from the same storage", () => {
    const first = renderSettings({
      expanded: [],
      bindings: [],
      task: { questionSwitches: allEnabled() },
    });
    fireEvent.click(row("waste"));
    const stored = first.storage.read("hb.settings.questions-expanded");
    first.unmount();

    // A reload is a fresh mount reading the storage the last one wrote.
    renderSettings({
      expanded: JSON.parse(stored ?? "[]") as string[],
      bindings: [],
      task: { questionSwitches: allEnabled() },
    });
    expect(isOpen("waste")).toBe(true);
  });

  it("shuts a row that was open, and leaves nothing behind when the last one shuts", () => {
    const { storage } = renderSettings({
      expanded: ["race"],
      bindings: [],
      task: { questionSwitches: allEnabled() },
    });
    expect(isOpen("race")).toBe(true);

    fireEvent.click(row("race"));

    expect(isOpen("race")).toBe(false);
    expect(storage.read("hb.settings.questions-expanded")).toBeNull();
  });

  it("sends the flipped state, naming the question, when the toggle is used", () => {
    const { onSetQuestionEnabled } = renderSettings({
      expanded: ["weekend"],
      bindings: [],
      task: { questionSwitches: allEnabled() },
    });

    fireEvent.click(screen.getByRole("switch", { name: /weekend/i }));

    expect(onSetQuestionEnabled).toHaveBeenCalledTimes(1);
    expect(onSetQuestionEnabled).toHaveBeenCalledWith("weekend", false);
  });

  it("says a question is off while its row is still shut", () => {
    // Load-bearing: the roster is the only place an off question can be
    // seen at all (ADR-0034's consequences), so "off" must never be a fact
    // you have to expand a row to find.
    renderSettings({
      expanded: [],
      bindings: [],
      task: {
        questionSwitches: allEnabled().map((entry) =>
          entry.question === "weekend" ? { ...entry, enabled: false } : entry,
        ),
      },
    });

    expect(screen.getAllByText("off")).toHaveLength(1);
    expect(screen.queryAllByText("queued")).toEqual([]);
  });

  it("marks an unconfirmed toggle queued, while it is shut", () => {
    renderSettings({
      expanded: [],
      bindings: [],
      task: {
        questionSwitches: allEnabled().map((entry) =>
          entry.question === "race" ? { question: "race", enabled: false, pending: true } : entry,
        ),
      },
    });

    expect(screen.getAllByText("queued")).toHaveLength(1);
    expect(screen.getAllByText("off")).toHaveLength(1);
  });

  it("says so when a toggle write failed, on that question's own row", () => {
    renderSettings({
      expanded: ["race", "waste"],
      bindings: [],
      task: {
        questionSwitches: allEnabled(),
        lastQuestionSwitchWrite: {
          seed: "s-1",
          question: "race",
          kind: "failed",
          error: "the queue is full",
        },
      },
    });

    const alerts = screen.getAllByRole("alert").map((node) => node.textContent);
    expect(alerts).toEqual(["the queue is full"]);
  });

  it("draws no toggle at all before the switches have been read", () => {
    // `null` is "nobody has answered", not "everything is on" — a toggle
    // rendered from an unread list would state a fact about the workspace.
    renderSettings({
      expanded: [...questionRoster().map((entry) => entry.question)],
      bindings: [],
      task: { questionSwitches: null },
    });

    expect(screen.queryAllByRole("switch")).toEqual([]);
    // The questions themselves are still listed — the roster does not wait
    // on the switches.
    expect(row("race")).toBeDefined();
  });

  it("draws the toggle read-only when the host cannot write", () => {
    renderSettings({
      expanded: ["race"],
      bindings: [],
      withSetBinding: false,
      task: { questionSwitches: allEnabled() },
    });

    const toggle = screen.getByRole("switch", { name: /race/i }) as HTMLInputElement;
    expect(toggle.readOnly).toBe(true);
    fireEvent.click(toggle);
    expect(toggle.checked).toBe(true);
  });

  it("leaves the leftovers group a plain disclosure with no toggle", () => {
    // Not a question: nothing switches it, and giving it one would offer to
    // silence rows that belong to no question at all.
    renderSettings({
      expanded: ["other.settings-rows"],
      bindings: [bindingDTO({ key: "some-future-binding", known: false })],
      task: { questionSwitches: allEnabled() },
    });

    const leftovers = screen.getByRole("button", {
      expanded: true,
      name: "Other settings rows",
    });
    expect(leftovers).toBeDefined();
    expect(screen.getByText("some-future-binding")).toBeDefined();
    expect(screen.queryAllByRole("switch")).toEqual([]);
  });
});

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
    // #714: the question is named from the core's roster, by looking up
    // which question claims `trips-calendar` — not from the sentence that
    // used to be hand-written here.
    const vacation = questionRoster().find((entry) =>
      entry.bindings.includes("trips-calendar"),
    );
    expect(vacation).toBeDefined();
    expect(screen.getByText(vacation!.label, { selector: "em" })).toBeDefined();
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

// The calendar connection's three new pieces of state (the mobile pass). All
// three were shipped with a fixture field and no assertion, which is the
// precise shape of the failure `test/component.tsx` records: state that
// compiles, typechecks, and is rendered by nobody.
describe("SettingsScreen — the calendar connection's state", () => {
  it("says nothing about a connection nobody has attempted", () => {
    renderSettings();
    expect(
      screen.getByRole("button", { name: /poll google calendar on this device/i }),
    ).toBeDefined();
    // No error copy, and nothing claiming a failure that has not happened —
    // checked against every real message `connect-error.ts` can produce,
    // not a hand-picked few, so this cannot pass by accident of which words
    // happened to be chosen.
    for (const code of ALL_CONNECT_ERROR_CODES) {
      expect(screen.queryByText(connectErrorCopy(code).message)).toBeNull();
    }
  });

  it.each(ALL_CONNECT_ERROR_CODES)(
    "renders %s as a diagnosis AND a next action",
    (code) => {
      // Both halves matter: `connect-error.ts` pairs them precisely because an
      // error the reader cannot act on is just bad news. A screen that rendered
      // only `message` would pass a laxer test and help nobody. Every code the
      // client can actually produce is exercised here, so none can ship
      // without its copy rendering.
      renderSettings({ calendar: { connectError: code } });
      const { message, hint } = connectErrorCopy(code);
      expect(screen.getByText(message)).toBeDefined();
      expect(screen.getByText(hint)).toBeDefined();
    },
  );

  it("echoes an unrecognised code rather than swallowing it", () => {
    // The fallback arm in `connect-error.ts` — a code nobody has classified
    // still has to render real words, since the whole failure mode being
    // fixed is a button that says nothing.
    renderSettings({ calendar: { connectError: "some_code_nobody_has_seen" } });
    expect(screen.getByText(/some_code_nobody_has_seen/i)).toBeDefined();
  });

  it("disables the button while a connect attempt is in flight", () => {
    // #585: every attempt, silent or interactive, is one same-origin POST to
    // the authority (ADR-0028) — there is no popup and no page navigation to
    // survive any more, but the awaited request still needs to keep a second
    // press from starting a second one.
    const { onConnect } = renderSettings({ calendar: { connectPending: true } });
    const button = screen.getByRole("button", { name: /poll google calendar on this device/i });
    expect((button as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(button);
    expect(onConnect).not.toHaveBeenCalled();
  });

  it("tells a blocked device that the background renewal has stopped, without guessing a cause", () => {
    // The ordinary reconnect sentence would leave the reader waiting for a
    // recovery that is never coming — `remint-health.ts` has stopped trying.
    // #581: one sentence covers all five of `BLOCKING_ERRORS`, which have
    // five different causes, so it names none of them. It says only what the
    // per-code copy does not: that retrying has stopped and the last
    // snapshot stays up.
    renderSettings({
      calendar: { connected: true, needsReconnect: true, silentRemintBlocked: true },
    });
    expect(screen.getByText(/renewing it in the background has stopped working/i)).toBeDefined();
    expect(screen.getByText(/last snapshot is still showing/i)).toBeDefined();
    expect(screen.queryByText(/revoked refresh token/i)).toBeNull();
    expect(
      screen.getByRole("button", { name: /retry polling on this device/i }),
    ).toBeDefined();
  });

  it("leaves the cause to the per-code copy when the block is a credential never provisioned", () => {
    // #581's own sighting, on the installed iPad PWA: `authority_unconfigured`
    // means the credential was never provisioned, and the blocked sentence
    // used to guess "revoked" directly above the per-code copy saying the
    // server has none configured. Two stacked sentences that disagreed.
    renderSettings({
      calendar: {
        connected: true,
        needsReconnect: true,
        silentRemintBlocked: true,
        connectError: "authority_unconfigured",
      },
    });
    expect(screen.getByText(connectErrorCopy("authority_unconfigured").message)).toBeDefined();
    expect(screen.queryByText(/revoked refresh token/i)).toBeNull();
  });

  it("keeps the ordinary reconnect sentence while the silent path is still trying", () => {
    renderSettings({ calendar: { connected: true, needsReconnect: true } });
    expect(screen.getByText(/The credential no longer works\./i)).toBeDefined();
    expect(screen.queryByText(/has stopped working/i)).toBeNull();
  });
});

// #585: both calendar gates now key off whether this device holds a device
// token (`taskTokenState`), not off `VITE_GOOGLE_CLIENT_ID` — that variable
// is unread anywhere in `client/web` as of this slice.
describe("SettingsScreen — the device-token precondition for calendar (#585)", () => {
  it("renders an explanatory note in place of the picker when this device has no device token", () => {
    renderSettings({ taskTokenState: "unset" });
    expect(screen.getByText(/this device has no device token/i)).toBeDefined();
    expect(screen.queryByRole("checkbox")).toBeNull();
  });

  it("renders no google-calendar status card when this device has no device token", () => {
    renderSettings({ taskTokenState: "unset" });
    expect(screen.queryByText("google calendar")).toBeNull();
    expect(
      screen.queryByRole("button", { name: /poll google calendar on this device/i }),
    ).toBeNull();
  });

  it("renders the calendar section once a device token is present, with no client id involved", () => {
    renderSettings({ taskTokenState: "resting" });
    expect(screen.queryByText(/this device has no device token/i)).toBeNull();
    expect(
      screen.getByRole("button", { name: /poll google calendar on this device/i }),
    ).toBeDefined();
  });
});

// #707's SharedWorker diagnostic journal — the "shipped-UI-with-no-reader"
// gate the Agent Brief names explicitly: this asserts the two buttons
// exist AND are wired to the two protocol ops, not merely that the screen
// renders without throwing.
describe("SettingsScreen — #707's diagnostics journal controls", () => {
  it("renders Download diagnostics, Clear diagnostics, and Download mirror when the core is ready", () => {
    renderSettings();
    expect(screen.getByRole("button", { name: "Download mirror" })).toBeDefined();
    expect(screen.getByRole("button", { name: "Download diagnostics" })).toBeDefined();
    expect(screen.getByRole("button", { name: "Clear diagnostics" })).toBeDefined();
  });

  it("Download diagnostics calls onDownloadDiagnostics, never onClearDiagnostics", () => {
    const { onDownloadDiagnostics, onClearDiagnostics } = renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "Download diagnostics" }));
    expect(onDownloadDiagnostics).toHaveBeenCalledTimes(1);
    expect(onClearDiagnostics).not.toHaveBeenCalled();
  });

  it("Clear diagnostics calls onClearDiagnostics, never onDownloadDiagnostics", () => {
    const { onDownloadDiagnostics, onClearDiagnostics } = renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "Clear diagnostics" }));
    expect(onClearDiagnostics).toHaveBeenCalledTimes(1);
    expect(onDownloadDiagnostics).not.toHaveBeenCalled();
  });

  // Review round 1 of PR #736: both controls used to sit inside the
  // `status === "ready"` gate, which made the journal unexportable exactly
  // when the core never reaches ready — one of the main situations an
  // operator needs it (a hang or a wasm load failure during startup,
  // #704's own incident). `worker/ports.ts`'s `DiagnosticsPortHandler`
  // makes the request genuinely servable in both of these states now
  // (queued-then-delivered while loading; explicitly answered by a failed
  // core) — this proves the SCREEN no longer hides the controls either.
  describe("reachable regardless of core status", () => {
    it("renders both diagnostics controls, clickable, while the core is still loading", () => {
      const { onDownloadDiagnostics, onClearDiagnostics } = renderSettings({ status: "loading" });

      expect(screen.getByText(/local core is still loading/i)).toBeDefined();
      fireEvent.click(screen.getByRole("button", { name: "Download diagnostics" }));
      fireEvent.click(screen.getByRole("button", { name: "Clear diagnostics" }));

      expect(onDownloadDiagnostics).toHaveBeenCalledTimes(1);
      expect(onClearDiagnostics).toHaveBeenCalledTimes(1);
    });

    it("renders both diagnostics controls, clickable, when the core failed to start", () => {
      const { onDownloadDiagnostics, onClearDiagnostics } = renderSettings({ status: "error" });

      expect(screen.getByText(/local core failed to start/i)).toBeDefined();
      fireEvent.click(screen.getByRole("button", { name: "Download diagnostics" }));
      fireEvent.click(screen.getByRole("button", { name: "Clear diagnostics" }));

      expect(onDownloadDiagnostics).toHaveBeenCalledTimes(1);
      expect(onClearDiagnostics).toHaveBeenCalledTimes(1);
    });

    it("never shows the not-ready note once the core is ready", () => {
      renderSettings({ status: "ready" });
      expect(screen.queryByText(/local core is still loading/i)).toBeNull();
      expect(screen.queryByText(/local core failed to start/i)).toBeNull();
    });

    it("does not gate Download mirror the same way — it stays ready-only, since the mirror is a wasm-side read", () => {
      renderSettings({ status: "loading" });
      expect(screen.queryByRole("button", { name: "Download mirror" })).toBeNull();
    });
  });
});
