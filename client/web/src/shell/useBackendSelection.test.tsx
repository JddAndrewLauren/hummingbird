// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import { useBackendSelection } from "./useBackendSelection";

function fakeStorage(): Storage {
  const store = new Map<string, string>();
  return {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => store.set(key, value),
    removeItem: (key) => store.delete(key),
    clear: () => store.clear(),
    key: () => null,
    get length() {
      return store.size;
    },
  } as Storage;
}

function Harness({ storage }: { storage: Storage }) {
  const { selection, setSelection } = useBackendSelection(storage);
  return (
    <>
      <span data-testid="selection">{selection}</span>
      <button type="button" onClick={() => setSelection("cloud")}>
        pin cloud
      </button>
    </>
  );
}

describe("useBackendSelection", () => {
  it("defaults to Auto", () => {
    render(<Harness storage={fakeStorage()} />);
    expect(screen.getByTestId("selection").textContent).toBe("auto");
  });

  it("setSelection updates state and persists to the injected storage", () => {
    const storage = fakeStorage();
    render(<Harness storage={storage} />);

    fireEvent.click(screen.getByText("pin cloud"));

    expect(screen.getByTestId("selection").textContent).toBe("cloud");
    expect(storage.getItem("hb.backend-selection")).toBe("cloud");
  });

  /** A context with no `localStorage` at all. The hook resolves its default
   * lazily rather than evaluating a bare `localStorage` that throws on the
   * way in — the same guard `App.tsx` puts on the rail preference — and
   * changing the preference there must be a no-op, never an exception. */
  it("renders Auto and survives a change when no storage is available", () => {
    function NoStorageHarness() {
      const { selection, setSelection } = useBackendSelection(undefined);
      return (
        <>
          <span data-testid="selection">{selection}</span>
          <button type="button" onClick={() => setSelection("cloud")}>
            pin cloud
          </button>
        </>
      );
    }

    render(<NoStorageHarness />);
    expect(screen.getByTestId("selection").textContent).toBe("auto");

    fireEvent.click(screen.getByText("pin cloud"));

    // The choice still takes effect for this session; only persistence is
    // lost, which is all a missing storage can cost.
    expect(screen.getByTestId("selection").textContent).toBe("cloud");
  });
});
