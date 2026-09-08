// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "../test/component";
import { useDropboxLocalRoot } from "./useDropboxLocalRoot";

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
  const { localRoot, setLocalRoot } = useDropboxLocalRoot(storage);
  return (
    <>
      <span data-testid="root">{localRoot ?? "none"}</span>
      <button type="button" onClick={() => setLocalRoot("C:\\Dropbox")}>
        set root
      </button>
      <button type="button" onClick={() => setLocalRoot("   ")}>
        clear root
      </button>
    </>
  );
}

describe("useDropboxLocalRoot", () => {
  it("reads null when nothing is stored", () => {
    render(<Harness storage={fakeStorage()} />);
    expect(screen.getByTestId("root").textContent).toBe("none");
  });

  it("reads a previously stored root at mount", () => {
    const storage = fakeStorage();
    storage.setItem("hb.dropbox-local-root", "/Users/john/Library/CloudStorage/Dropbox");
    render(<Harness storage={storage} />);
    expect(screen.getByTestId("root").textContent).toBe(
      "/Users/john/Library/CloudStorage/Dropbox",
    );
  });

  it("setLocalRoot persists to the injected storage and re-reads it into state", () => {
    const storage = fakeStorage();
    render(<Harness storage={storage} />);

    fireEvent.click(screen.getByText("set root"));

    expect(screen.getByTestId("root").textContent).toBe("C:\\Dropbox");
    expect(storage.getItem("hb.dropbox-local-root")).toBe("C:\\Dropbox");
  });

  it("a blank value clears the stored root so it reads as never set", () => {
    const storage = fakeStorage();
    storage.setItem("hb.dropbox-local-root", "C:\\Dropbox");
    render(<Harness storage={storage} />);
    expect(screen.getByTestId("root").textContent).toBe("C:\\Dropbox");

    fireEvent.click(screen.getByText("clear root"));

    expect(screen.getByTestId("root").textContent).toBe("none");
    expect(storage.getItem("hb.dropbox-local-root")).toBeNull();
  });
});
