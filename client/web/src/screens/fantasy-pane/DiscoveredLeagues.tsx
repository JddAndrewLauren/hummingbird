import type { FantasyDiscoveredCore } from "../../decisions/seam";

// The leagues the poller found for this season (#855), drawn under the
// `yahoo-leagues` field in Settings. League keys are season-scoped, so the
// binding goes stale every August; this list makes the re-type a copy rather
// than a hunt (#713's binding section). The read is the core's
// (`fantasy.rs`'s `fantasy_discovered_leagues`); this is only its words.
//
// No row is nothing to say — the poller has not run, or this device has not
// read the source yet — so that arm draws nothing at all.

export function DiscoveredLeagues({ discovered }: { discovered: FantasyDiscoveredCore }) {
  if (discovered.kind === "none") {
    return null;
  }
  if (discovered.kind === "unreadable") {
    return (
      <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
        This season's leagues couldn't be read.
      </p>
    );
  }
  if (discovered.leagues.length === 0) {
    return (
      <p style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
        No leagues found this season.
      </p>
    );
  }
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
      <span className="hb-meta">found this season</span>
      <ul
        aria-label="Leagues found this season"
        style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: "var(--space-2)" }}
      >
        {discovered.leagues.map((league) => (
          <li
            key={league.leagueKey}
            style={{ display: "flex", alignItems: "baseline", gap: "var(--space-4)", flexWrap: "wrap" }}
          >
            <span style={{ font: "var(--type-body-sm)", color: "var(--text-primary)" }}>{league.leagueName}</span>
            {/* Selectable as-is, and never uppercased: this is the exact text
                to copy into the field, and a key's `.l.` is lowercase. */}
            <span className="hb-meta" style={{ userSelect: "all", textTransform: "none" }}>
              {league.leagueKey}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
