import { describe, expect, it, vi } from "vitest";
import { mintCoreId } from "./core-id";

// The arm that matters is the one WITHOUT `crypto.randomUUID`: it is
// unreachable in every environment this is normally run in (https, and
// localhost), and reachable in the one a device test uses — the app served
// over a LAN IP, which is not a secure context. Called bare there,
// `crypto.randomUUID()` throws at `core.worker.ts`'s module scope, before
// `self.onconnect` is assigned, and every view hangs on "Loading core…"
// forever. A diagnostic must not be able to do that.

describe("mintCoreId", () => {
  it("uses crypto.randomUUID when there is one, shortened to something readable", () => {
    const source = {
      randomUUID: () => "01d13673-1f2e-4a5b-8c9d-0e1f2a3b4c5d",
    } as unknown as Crypto;

    expect(mintCoreId(source)).toBe("01d13673");
  });

  it("still mints an id where crypto.randomUUID does not exist, rather than throwing", () => {
    // `{}` is the shape a non-secure-context worker scope actually presents:
    // `crypto` is there, `randomUUID` is not.
    const source = {} as Crypto;

    expect(() => mintCoreId(source)).not.toThrow();
    expect(mintCoreId(source)).toHaveLength(8);
  });

  it("still mints an id where there is no crypto at all", () => {
    expect(mintCoreId(undefined)).toHaveLength(8);
  });

  it("mints a different id per core, on the fallback arm too", () => {
    // Two cores alive on one device at one moment are the only ids that
    // must differ — the whole probe is reading one against the other.
    const source = {} as Crypto;

    expect(mintCoreId(source)).not.toEqual(mintCoreId(source));
  });

  it("reads the ambient crypto when the caller names none — core.worker.ts's own call", () => {
    const randomUUID = vi.fn(() => "aa11bb22-0000-0000-0000-000000000000");
    vi.stubGlobal("crypto", { randomUUID });

    expect(mintCoreId()).toBe("aa11bb22");
    expect(randomUUID).toHaveBeenCalled();

    vi.unstubAllGlobals();
  });
});
