import { describe, expect, it } from "vitest";
import { deviceRows, signOutConfirmation } from "./seam";
import type { DeviceInfoDTO } from "../store/protocol";

const HOUR = 3_600_000;

function device(id: string, overrides: Partial<DeviceInfoDTO> = {}): DeviceInfoDTO {
  return {
    id,
    name: `${id} name`,
    platform: "web",
    created_at: 0,
    last_seen: null,
    revoked_at: null,
    is_current: false,
    revocable: true,
    ...overrides,
  };
}

describe("deviceRows (#840 over the core's devices decision)", () => {
  it("puts this device first, then the most recently seen, and drops revoked rows", () => {
    const rows = deviceRows(
      [
        device("device-web-old", { last_seen: 1 * HOUR }),
        device("device-android-pixel", { platform: "android", last_seen: 9 * HOUR }),
        device("device-web-gone", { revoked_at: 5 * HOUR }),
        device("device-web-here", { is_current: true, last_seen: 2 * HOUR }),
        device("runner", { platform: "other", revocable: false, last_seen: 3 * HOUR }),
      ],
      10 * HOUR,
    );
    expect(rows.map((row) => row.id)).toEqual([
      "device-web-here",
      "device-android-pixel",
      "runner",
      "device-web-old",
    ]);
    expect(rows[0]).toMatchObject({ isCurrent: true, thisDevice: "This device", platformLabel: "Web" });
    expect(rows[1]).toMatchObject({ thisDevice: null, platformLabel: "Android", lastSeenLabel: "Last seen 1h ago" });
    expect(rows[2].revocable).toBe(false);
  });
});

describe("signOutConfirmation", () => {
  it("names how many changes stay queued", () => {
    expect(signOutConfirmation(0)).toBe("Sign out this device? Nothing is waiting to send.");
    expect(signOutConfirmation(3)).toContain("3 unsent changes stay queued");
  });
});
