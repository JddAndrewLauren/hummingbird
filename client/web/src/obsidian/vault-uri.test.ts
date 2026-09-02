import { describe, expect, it } from "vitest";
import type { BindingDTO } from "../store/protocol";
import {
  buildUri,
  derivePath,
  isValidVaultPath,
  obsidianVaultName,
  OBSIDIAN_VAULT_BINDING_KEY,
} from "./vault-uri";

function binding(key: string, value: BindingDTO["value"]): BindingDTO {
  return { key, known: true, pending: false, value };
}

describe("buildUri", () => {
  // The assertion this whole module exists to keep. Verified against a real
  // vault: without `&append`, firing at an existing note creates
  // `<name> 1.md` beside it and the item points at a note it never wrote.
  // See the module header for the full smoke-test table.
  it("keeps the &append flag, which is what makes the call idempotent", () => {
    expect(buildUri("JDD", "Hummingbird/Knee rehab.md")).toContain("&append");
  });

  it("escapes the vault and the path per Obsidian's own rules", () => {
    expect(buildUri("JDD", "Hummingbird/Knee rehab.md")).toBe(
      "obsidian://new?vault=JDD&file=Hummingbird%2FKnee%20rehab.md&append",
    );
  });

  it("escapes a vault name that is not a bare word", () => {
    expect(buildUri("My Vault & co", "a.md")).toBe(
      "obsidian://new?vault=My%20Vault%20%26%20co&file=a.md&append",
    );
  });
});

describe("derivePath", () => {
  it("puts the note in one folder, so the whole feature is one delete away", () => {
    expect(derivePath("Knee rehab")).toBe("Hummingbird/Knee rehab.md");
  });

  it("strips the characters Obsidian will not accept in a file name", () => {
    expect(derivePath('Fix the *broken* "thing": why?')).toBe(
      "Hummingbird/Fix the broken thing why.md",
    );
  });

  it("is deterministic, so a re-click re-points at the note it had", () => {
    expect(derivePath("Knee rehab")).toBe(derivePath("Knee rehab"));
  });

  it("proposes nothing for a title that strips to an empty name", () => {
    // `???` passes the form (it is not blank) but leaves no basename, and
    // `Hummingbird/.md` is a hidden note every such item would share.
    expect(derivePath("???")).toBe(null);
    expect(derivePath('  "" ')).toBe(null);
  });
});

describe("isValidVaultPath", () => {
  it("accepts a path with no extension — .md is not required", () => {
    // The `file` parameter allows omitting the extension, and the vault also
    // holds `.canvas` and `.base` files.
    expect(isValidVaultPath("Reading/Knee rehab")).toBe(true);
    expect(isValidVaultPath("Boards/Trip.canvas")).toBe(true);
  });

  it("rejects a blank path — clearing the pointer is a null, never an empty string", () => {
    expect(isValidVaultPath("")).toBe(false);
    expect(isValidVaultPath("   ")).toBe(false);
  });

  it("rejects anything that tries to leave the vault", () => {
    expect(isValidVaultPath("/Users/john/secrets.md")).toBe(false);
    expect(isValidVaultPath("../outside.md")).toBe(false);
    expect(isValidVaultPath("Hummingbird/../../outside.md")).toBe(false);
  });

  it("does not mistake a name that merely contains dots for a traversal", () => {
    expect(isValidVaultPath("Hummingbird/v1..2 notes.md")).toBe(true);
  });
});

describe("obsidianVaultName", () => {
  it("reads the bound vault name", () => {
    expect(
      obsidianVaultName([binding(OBSIDIAN_VAULT_BINDING_KEY, { state: "text", text: "JDD" })]),
    ).toBe("JDD");
  });

  it("collapses every not-a-vault-name input to null", () => {
    expect(obsidianVaultName(null)).toBe(null);
    expect(obsidianVaultName([])).toBe(null);
    expect(obsidianVaultName([binding(OBSIDIAN_VAULT_BINDING_KEY, { state: "unset" })])).toBe(null);
    expect(
      obsidianVaultName([binding(OBSIDIAN_VAULT_BINDING_KEY, { state: "text", text: "  " })]),
    ).toBe(null);
    expect(
      obsidianVaultName([binding(OBSIDIAN_VAULT_BINDING_KEY, { state: "other", raw: "7" })]),
    ).toBe(null);
  });

  it("trims, so a pasted name with a stray space still opens the vault", () => {
    expect(
      obsidianVaultName([binding(OBSIDIAN_VAULT_BINDING_KEY, { state: "text", text: " JDD " })]),
    ).toBe("JDD");
  });
});
