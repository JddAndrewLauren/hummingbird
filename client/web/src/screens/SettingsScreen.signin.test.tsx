// @vitest-environment jsdom

// #840 (ADR-0040 decisions 3 and 7): the device-token card leads with Sign
// in with Google and keeps pasting behind "Paste a token instead", and the
// Signed-in devices section draws the core's rows with their sign-outs.
// Mounted for `test/component.tsx`'s reason — the wiring under these props
// is tested in `shell/useSignInWiring.test.tsx` and
// `shell/useDevicesWiring.test.tsx`; this pins that the screen reads them.
// The rows go through the REAL decision seam, so their order and words are
// the core's own.

import { describe, expect, it, vi } from "vitest";

import { SettingsScreen } from "./SettingsScreen";
import { fireEvent, render, screen, taskState, within } from "../test/component";
import type { DeviceInfoDTO, DevicesDTO } from "../store/protocol";
import type { CalendarState, TaskState } from "../store/store";
import type { SignInUiState } from "../shell/useSignInWiring";
import type { TaskTokenUiState } from "../task/token-ui";

const HOUR = 3_600_000;
const NOW = 100 * HOUR;

const calendar: CalendarState = {
  connected: false,
  needsReconnect: false,
  availableCalendars: [],
  lastPollOutcome: null,
  connectPending: false,
  connectError: null,
  silentRemintBlocked: false,
  eventReads: {},
};

function device(id: string, overrides: Partial<DeviceInfoDTO> = {}): DeviceInfoDTO {
  return {
    id,
    name: id,
    platform: "web",
    created_at: 0,
    last_seen: null,
    revoked_at: null,
    is_current: false,
    revocable: true,
    ...overrides,
  };
}

const LISTED: DevicesDTO = {
  kind: "listed",
  error: null,
  devices: [
    device("Phone", { platform: "android", last_seen: NOW - 3 * HOUR }),
    device("Skill runner", { platform: "other", last_seen: NOW - HOUR, revocable: false }),
    device("Old laptop", { revoked_at: NOW - 50 * HOUR }),
    device("This browser", { is_current: true, last_seen: NOW - 1_000 }),
  ],
};

interface Options {
  taskTokenState?: TaskTokenUiState;
  signIn?: SignInUiState;
  onBeginSignIn?: (() => void) | null;
  task?: Partial<TaskState>;
  onRevokeDevice?: (id: string) => Promise<string | null>;
}

function renderSettings(options: Options = {}) {
  const onSubmitTaskToken = vi.fn().mockResolvedValue("ok");
  const tree = (current: Options) => (
    <SettingsScreen
      status="ready"
      apiVersion={1}
      coreId={null}
      viewOrdinal={null}
      error={null}
      calendar={calendar}
      calendarIsDemo={false}
      themePreference="system"
      onThemePreference={vi.fn()}
      backendSelection="auto"
      onBackendSelection={vi.fn()}
      dropboxLocalRoot={null}
      onDropboxLocalRoot={vi.fn()}
      onConnect={vi.fn()}
      onToggleCalendar={vi.fn()}
      onRefresh={vi.fn()}
      taskTokenState={current.taskTokenState ?? "unset"}
      taskTokenEnteredAtMs={null}
      onSubmitTaskToken={onSubmitTaskToken}
      onForgetTaskToken={vi.fn()}
      signIn={current.signIn}
      onBeginSignIn={current.onBeginSignIn}
      onRevokeDevice={current.onRevokeDevice}
      task={taskState(current.task)}
      online
      syncNowMs={NOW}
      onDownloadMirror={vi.fn()}
      onDownloadDiagnostics={vi.fn()}
      onClearDiagnostics={vi.fn()}
    />
  );
  const { rerender } = render(tree(options));
  return { onSubmitTaskToken, update: (next: Options) => rerender(tree(next)) };
}

function devicesSection(): HTMLElement {
  return screen.getByRole("heading", { name: "Signed-in devices" }).parentElement!.parentElement!;
}

describe("the device-token card's sign-in", () => {
  it("leads with Sign in with Google and keeps the paste form shut behind its disclosure", () => {
    const onBeginSignIn = vi.fn();
    const { onSubmitTaskToken } = renderSettings({ onBeginSignIn });

    fireEvent.click(screen.getByRole("button", { name: "Sign in with Google" }));
    expect(onBeginSignIn).toHaveBeenCalledTimes(1);

    expect(screen.queryByLabelText("Device token")).toBeNull();
    const disclosure = screen.getByRole("button", { name: "Paste a token instead" });
    expect(disclosure.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(disclosure);
    expect(disclosure.getAttribute("aria-expanded")).toBe("true");

    // The fallback works exactly as the paste form always has.
    fireEvent.change(screen.getByLabelText("Device token"), { target: { value: "hb_pasted" } });
    fireEvent.click(screen.getByRole("button", { name: "Save token" }));
    expect(onSubmitTaskToken).toHaveBeenCalledWith("hb_pasted");
  });

  it("re-prompts a held token with the same sign-in in front", () => {
    renderSettings({ taskTokenState: "reprompt", onBeginSignIn: vi.fn() });
    expect(screen.getByRole("button", { name: "Sign in with Google" })).toBeDefined();
  });

  it("draws the paste form alone on a build with no sign-in client", () => {
    renderSettings({ onBeginSignIn: null });
    expect(screen.queryByRole("button", { name: "Sign in with Google" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Paste a token instead" })).toBeNull();
    expect(screen.getByLabelText("Device token")).toBeDefined();
  });

  it("shows a sign-in in progress, and a failed one's reason", () => {
    const { update } = renderSettings({ onBeginSignIn: vi.fn(), signIn: { kind: "pending" } });
    const pending = screen.getByRole("button", { name: /signing in/i }) as HTMLButtonElement;
    expect(pending.disabled).toBe(true);

    update({
      onBeginSignIn: vi.fn(),
      signIn: { kind: "error", message: "Sign-in was cancelled." },
    });
    expect(screen.getByRole("alert").textContent).toBe("Sign-in was cancelled.");
    expect(screen.getByRole("button", { name: "Sign in with Google" })).toBeDefined();
  });

  it("offers no sign-in to a device already holding a working token", () => {
    renderSettings({ taskTokenState: "resting", onBeginSignIn: vi.fn() });
    expect(screen.queryByRole("button", { name: "Sign in with Google" })).toBeNull();
    expect(screen.getByRole("button", { name: "Forget token" })).toBeDefined();
  });
});

describe("Signed-in devices", () => {
  it("lists this device first as This device, then the most recently seen, and never a revoked one", () => {
    renderSettings({ taskTokenState: "resting", task: { devices: LISTED }, onRevokeDevice: vi.fn() });
    const rows = within(devicesSection()).getAllByRole("listitem");
    expect(rows.map((row) => row.getAttribute("aria-label"))).toEqual([
      "This browser, This device",
      "Skill runner",
      "Phone",
    ]);
    expect(within(rows[0]).getByText("This device")).toBeDefined();
    expect(within(rows[2]).getByText("Android · Last seen 3h ago")).toBeDefined();
  });

  it("gives each revocable row its sign-out and the others none", () => {
    renderSettings({ taskTokenState: "resting", task: { devices: LISTED }, onRevokeDevice: vi.fn() });
    const [current, runner, phone] = within(devicesSection()).getAllByRole("listitem");
    expect(within(current).getByRole("button", { name: "Sign out of this device" })).toBeDefined();
    expect(within(runner).queryByRole("button")).toBeNull();
    expect(within(phone).getByRole("button", { name: "Sign out" })).toBeDefined();
  });

  it("confirms this device's sign-out with the core's queued-changes warning", async () => {
    const onRevokeDevice = vi.fn().mockResolvedValue(null);
    renderSettings({
      taskTokenState: "resting",
      task: { devices: LISTED, queueDepth: 2 },
      onRevokeDevice,
    });
    const [current] = within(devicesSection()).getAllByRole("listitem");
    fireEvent.click(within(current).getByRole("button", { name: "Sign out of this device" }));
    expect(onRevokeDevice).not.toHaveBeenCalled();
    expect(
      within(current).getByText(
        "Sign out this device? 2 unsent changes stay queued and send once you sign in again.",
      ),
    ).toBeDefined();

    fireEvent.click(within(current).getByRole("button", { name: "Confirm" }));
    expect(onRevokeDevice).toHaveBeenCalledWith("This browser");
  });

  it("confirms another device's sign-out by name, and Cancel takes it back", () => {
    const onRevokeDevice = vi.fn().mockResolvedValue(null);
    renderSettings({ taskTokenState: "resting", task: { devices: LISTED }, onRevokeDevice });
    const phone = within(devicesSection()).getAllByRole("listitem")[2];
    fireEvent.click(within(phone).getByRole("button", { name: "Sign out" }));
    expect(within(phone).getByText("Sign out Phone? It stops syncing until it signs in again.")).toBeDefined();
    fireEvent.click(within(phone).getByRole("button", { name: "Cancel" }));
    expect(within(phone).getByRole("button", { name: "Sign out" })).toBeDefined();
    expect(onRevokeDevice).not.toHaveBeenCalled();
  });

  it("says why a sign-out did not happen, on its row", async () => {
    const onRevokeDevice = vi.fn().mockResolvedValue("That device is no longer signed in.");
    renderSettings({ taskTokenState: "resting", task: { devices: LISTED }, onRevokeDevice });
    const phone = within(devicesSection()).getAllByRole("listitem")[2];
    fireEvent.click(within(phone).getByRole("button", { name: "Sign out" }));
    fireEvent.click(within(phone).getByRole("button", { name: "Confirm" }));
    expect((await within(phone).findByRole("alert")).textContent).toBe("That device is no longer signed in.");
  });

  it("returns to the signed-out state once this device has signed out", () => {
    const { update } = renderSettings({
      taskTokenState: "resting",
      task: { devices: LISTED },
      onRevokeDevice: vi.fn(),
      onBeginSignIn: vi.fn(),
    });
    update({
      taskTokenState: "unset",
      task: { devices: { kind: "no_credential", devices: [], error: null } },
      onRevokeDevice: vi.fn(),
      onBeginSignIn: vi.fn(),
    });
    expect(within(devicesSection()).queryAllByRole("listitem")).toHaveLength(0);
    expect(
      within(devicesSection()).getByText(
        "This device is signed out. Sign in to see the devices signed in to hummingbird.",
      ),
    ).toBeDefined();
    expect(screen.getByRole("button", { name: "Sign in with Google" })).toBeDefined();
  });

  it("says it is reading before the first answer", () => {
    renderSettings({ task: { devices: null } });
    expect(within(devicesSection()).getByText("Reading your devices.")).toBeDefined();
  });
});
