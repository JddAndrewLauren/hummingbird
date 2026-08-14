import { describe, expect, it } from "vitest";
import { itemDTO } from "../test/component";
import { deadLetterSubject } from "./dead-letter-subject";
import type { LedgerRowDTO } from "../store/protocol";

const ledgerRow = (id: string, title: string): LedgerRowDTO => ({
  ...itemDTO({ id, title, stage: "ready" }),
  absentSinceMs: null,
  deadLettered: true,
  hasLiveAlert: false,
});

const ledger = [ledgerRow("a-1", "Ring the plumber")];

describe("deadLetterSubject", () => {
  it("names an item by its title — the only spelling a person can act on", () => {
    expect(deadLetterSubject({ entity: "items", entityId: "a-1" }, ledger)).toBe(
      'item "Ring the plumber"',
    );
  });

  it("names an item that has since been archived — the roster retains it", () => {
    // The point of reading `ledger` rather than the frontier: a dead-lettered
    // edit outlives the item's life on the board, and the complete retained
    // roster is what still holds the title. An archived row naming nothing
    // would strand exactly the entries most likely to need re-applying by
    // hand.
    const archived: LedgerRowDTO = {
      ...ledgerRow("a-2", "Cancel the gym membership"),
      absentSinceMs: 5_000,
      archivedAt: 5_000,
    };
    expect(deadLetterSubject({ entity: "items", entityId: "a-2" }, [...ledger, archived])).toBe(
      'item "Cancel the gym membership"',
    );
  });

  it("names an item the ledger has never heard of by its id, not by nothing", () => {
    expect(deadLetterSubject({ entity: "items", entityId: "a-9" }, ledger)).toBe("item a-9");
  });

  it("falls back to the id while the ledger has not been read yet", () => {
    // `null` is "not read yet", not "no such item" — so this must not read as
    // a confident "that item does not exist".
    expect(deadLetterSubject({ entity: "items", entityId: "a-1" }, null)).toBe("item a-1");
  });

  it("names other entities by id — no title lookup exists for them", () => {
    expect(deadLetterSubject({ entity: "steps", entityId: "s-1" }, ledger)).toBe("step s-1");
    expect(deadLetterSubject({ entity: "settings", entityId: "theme" }, ledger)).toBe(
      "setting theme",
    );
  });

  it("uses the path segment verbatim for an entity with no display word", () => {
    expect(deadLetterSubject({ entity: "projects", entityId: "p-1" }, ledger)).toBe(
      "projects p-1",
    );
  });

  it("says the entity alone when the intent named no row, inventing no identity", () => {
    expect(deadLetterSubject({ entity: "items", entityId: null }, ledger)).toBe("item");
    expect(deadLetterSubject({ entity: "fog", entityId: null }, ledger)).toBe("fog");
  });
});
