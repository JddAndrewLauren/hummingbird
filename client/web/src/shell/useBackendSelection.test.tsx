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
});
