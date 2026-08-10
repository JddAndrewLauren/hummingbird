import { describe, expect, it } from "vitest";
import { mirrorSnapshotFilename } from "./mirror-download";

describe("mirrorSnapshotFilename", () => {
  it("embeds a filesystem-safe timestamp with no colons or dots", () => {
    const name = mirrorSnapshotFilename(Date.UTC(2026, 7, 9, 12, 34, 56));
    expect(name).toBe("hummingbird-mirror-2026-08-09T12-34-56-000Z.json");
    expect(name).not.toMatch(/[:.](?!json$)/);
  });

  it("two different instants produce two different filenames", () => {
    const first = mirrorSnapshotFilename(1_000);
    const second = mirrorSnapshotFilename(2_000);
    expect(first).not.toBe(second);
  });
});
