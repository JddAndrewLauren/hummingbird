// @vitest-environment jsdom

// `useIsPhone` chooses between two different DOM trees (the rail and the
// bottom bar), so both branches have to be real. The guard is the part worth
// pinning hardest: this repo's jsdom does not implement `matchMedia` at all,
// and an unguarded call throws inside render — which is not a hypothetical,
// it took down 21 of `CapturePopover`'s tests when this hook first landed.

import { afterEach, describe, expect, it, vi } from "vitest";
import { act, render, screen } from "../test/component";
import { PHONE_QUERY } from "./breakpoints";
import { useIsPhone } from "./useIsPhone";

function Probe() {
  return <span data-testid="probe">{useIsPhone() ? "phone" : "desktop"}</span>;
}

function readProbe(): string {
  return screen.getByTestId("probe").textContent ?? "";
}

/** A `matchMedia` that answers `matches` and records its listeners, so the
 * subscription can be driven and its teardown checked. */
function stubMatchMedia(matches: boolean) {
  const listeners = new Set<() => void>();
  let current = matches;
  vi.stubGlobal("matchMedia", (query: string) => {
    expect(query).toBe(PHONE_QUERY);
    return {
      get matches() {
        return current;
      },
      addEventListener: (_event: string, listener: () => void) => listeners.add(listener),
      removeEventListener: (_event: string, listener: () => void) => listeners.delete(listener),
    };
  });
  return {
    listenerCount: () => listeners.size,
    resizeTo(next: boolean) {
      current = next;
      act(() => {
        for (const listener of [...listeners]) {
          listener();
        }
      });
    },
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("useIsPhone", () => {
  it("reads the desktop branch where the query does not match", () => {
    stubMatchMedia(false);
    render(<Probe />);
    expect(readProbe()).toBe("desktop");
  });

  it("reads the phone branch where it does", () => {
    stubMatchMedia(true);
    render(<Probe />);
    expect(readProbe()).toBe("phone");
  });

  it("follows a resize across the breakpoint, in both directions", () => {
    // Not academic: rotating a tablet, or dragging a desktop window narrow,
    // must swap the nav rather than leave a rail on a 390px viewport.
    const media = stubMatchMedia(false);
    render(<Probe />);
    media.resizeTo(true);
    expect(readProbe()).toBe("phone");
    media.resizeTo(false);
    expect(readProbe()).toBe("desktop");
  });

  it("unsubscribes on unmount", () => {
    const media = stubMatchMedia(true);
    const { unmount } = render(<Probe />);
    expect(media.listenerCount()).toBe(1);
    unmount();
    expect(media.listenerCount()).toBe(0);
  });

  // The guard, stated as a test so nobody "simplifies" it away. Absent
  // `matchMedia` is read as "not a phone" — the honest answer to a viewport
  // question a runtime cannot answer, and the one every existing component
  // test depends on, since they all assert the desktop tree.
  it("does not throw where matchMedia does not exist, and reports desktop", () => {
    vi.stubGlobal("matchMedia", undefined);
    expect(() => render(<Probe />)).not.toThrow();
    expect(readProbe()).toBe("desktop");
  });
});
