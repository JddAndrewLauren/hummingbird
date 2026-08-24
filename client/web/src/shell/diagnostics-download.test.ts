import { describe, expect, it } from "vitest";
import { diagnosticsExportFilename } from "./diagnostics-download";

describe("diagnosticsExportFilename", () => {
  it("embeds a filesystem-safe timestamp with no colons or dots", () => {
    const name = diagnosticsExportFilename(Date.UTC(2026, 7, 9, 12, 34, 56));
    expect(name).toBe("hummingbird-diagnostics-2026-08-09T12-34-56-000Z.json");
    expect(name).not.toMatch(/[:.](?!json$)/);
  });

  it("two different instants produce two different filenames", () => {
    const first = diagnosticsExportFilename(1_000);
    const second = diagnosticsExportFilename(2_000);
    expect(first).not.toBe(second);
  });

  it("never collides with the mirror's own filename scheme", () => {
    const name = diagnosticsExportFilename(1_000);
    expect(name).not.toContain("hummingbird-mirror");
  });
});
