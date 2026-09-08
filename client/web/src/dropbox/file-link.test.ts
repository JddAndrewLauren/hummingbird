import { describe, expect, it } from "vitest";
import {
  buildDropboxWebUrl,
  buildOpenUri,
  isValidFilePath,
  normalizePastedPath,
  normalizeSeparators,
  splitFilePath,
} from "./file-link";

describe("buildOpenUri", () => {
  it("fires the bare-query scheme, never a // host form", () => {
    expect(buildOpenUri("Finance/2026/receipt.pdf")).toBe(
      "hummingbird-open:?path=Finance%2F2026%2Freceipt.pdf",
    );
  });

  it("escapes a space the way both helpers decode it", () => {
    expect(buildOpenUri("House/Tap washer.pdf")).toBe("hummingbird-open:?path=House%2FTap%20washer.pdf");
  });
});

describe("buildDropboxWebUrl", () => {
  it("previews a file inside its folder", () => {
    expect(buildDropboxWebUrl("Finance/2026/receipt.pdf")).toBe(
      "https://www.dropbox.com/home/Finance/2026?preview=receipt.pdf",
    );
  });

  it("links a folder as the folder itself", () => {
    expect(buildDropboxWebUrl("House/Plumbing")).toBe("https://www.dropbox.com/home/House/Plumbing");
  });

  it("previews a root-level file with an empty folder", () => {
    expect(buildDropboxWebUrl("receipt.pdf")).toBe("https://www.dropbox.com/home/?preview=receipt.pdf");
  });

  it("encodes each segment", () => {
    expect(buildDropboxWebUrl("A b/c&d.pdf")).toBe("https://www.dropbox.com/home/A%20b?preview=c%26d.pdf");
  });
});

describe("normalizePastedPath", () => {
  it("turns a Windows Copy-as-path into a Dropbox-relative path", () => {
    expect(normalizePastedPath('"C:\\Dropbox\\Finance\\2026\\receipt.pdf"', "C:\\Dropbox")).toBe(
      "Finance/2026/receipt.pdf",
    );
  });

  it("strips a Mac root with or without its trailing slash, case-insensitively", () => {
    const root = "/Users/john/Library/CloudStorage/Dropbox/";
    expect(normalizePastedPath("/users/john/library/cloudstorage/dropbox/House/Plumbing", root)).toBe(
      "House/Plumbing",
    );
  });

  it("leaves a relative path alone and only flips its slashes", () => {
    expect(normalizePastedPath("Finance\\receipt.pdf", "C:\\Dropbox")).toBe("Finance/receipt.pdf");
  });

  it("does not strip a root that merely prefixes a longer folder name", () => {
    expect(normalizePastedPath("C:/Dropbox2/x.pdf", "C:/Dropbox")).toBe("C:/Dropbox2/x.pdf");
  });

  it("with no root known, an absolute paste stays absolute (and then invalid)", () => {
    const path = normalizePastedPath('"C:\\Dropbox\\x.pdf"', null);
    expect(path).toBe("C:/Dropbox/x.pdf");
    expect(isValidFilePath(path)).toBe(false);
  });
});

describe("isValidFilePath", () => {
  it("accepts a file and a folder alike — no extension rule", () => {
    expect(isValidFilePath("Finance/2026/receipt.pdf")).toBe(true);
    expect(isValidFilePath("House/Plumbing")).toBe(true);
  });

  it("rejects blank", () => {
    expect(isValidFilePath("")).toBe(false);
    expect(isValidFilePath("   ")).toBe(false);
  });

  it("rejects every way of leaving Dropbox", () => {
    expect(isValidFilePath("/etc/passwd")).toBe(false);
    expect(isValidFilePath("../secrets.txt")).toBe(false);
    expect(isValidFilePath("a/../b.pdf")).toBe(false);
    expect(isValidFilePath("C:/Dropbox/x.pdf")).toBe(false);
    expect(isValidFilePath("~/x.pdf")).toBe(false);
  });

  it("does not mistake a name that contains dots for a traversal", () => {
    expect(isValidFilePath("Reports/2026..draft.pdf")).toBe(true);
    expect(isValidFilePath("Reports/.hidden")).toBe(true);
  });

  it("reads a back-slash as a separator, so a traversal cannot hide behind one", () => {
    expect(isValidFilePath("a\\..\\b.pdf")).toBe(false);
    expect(isValidFilePath("..\\secrets.txt")).toBe(false);
    expect(isValidFilePath("\\etc\\passwd")).toBe(false);
    expect(isValidFilePath("Finance\\2026\\receipt.pdf")).toBe(true);
  });

  it("rejects an empty or a . segment", () => {
    expect(isValidFilePath("a//b.pdf")).toBe(false);
    expect(isValidFilePath("House/Plumbing/")).toBe(false);
    expect(isValidFilePath("./receipt.pdf")).toBe(false);
    expect(isValidFilePath("a/./b.pdf")).toBe(false);
  });
});

describe("normalizeSeparators", () => {
  it("turns every back-slash into a forward slash and touches nothing else", () => {
    expect(normalizeSeparators("Finance\\2026\\receipt.pdf")).toBe("Finance/2026/receipt.pdf");
    expect(normalizeSeparators("Finance/2026/receipt.pdf")).toBe("Finance/2026/receipt.pdf");
  });
});

describe("splitFilePath", () => {
  it("names the basename and the folder", () => {
    expect(splitFilePath("Finance/2026/receipt.pdf")).toEqual({ name: "receipt.pdf", folder: "Finance/2026" });
  });

  it("a root-level path has no folder", () => {
    expect(splitFilePath("receipt.pdf")).toEqual({ name: "receipt.pdf", folder: null });
  });
});
