# `server/fantasy-poll` fixtures

JSON carries no comments, so the headers these files would otherwise have
live here.

## `roster.json` and `teams.json`

**This crate's own typed inputs, not Yahoo's wire format.** They are
`roster::Roster` and a list of `roster::Team`. The parse from Yahoo into these
types is #854's, and it will pin itself against a saved sample of the real
response. The names and keys are invented. The kickoffs are real week-4 2026
instants: Thursday night, a Sunday 9:30am ET London game, Sunday 1pm and
4:25pm, Sunday and Monday night.

`roster.json` is built so that each validity rule and the deadline rule have
a witness. The starters include a WR on bye, an empty RB slot, a W/R/T
starter who is Out, and a questionable RB who stays valid. On the bench, a
WR in Thursday's game holds the earliest eligible-replacement kickoff, so
it sets the lineup's deadline.

## `roster-starter-locks-first.json`

The operator's #853 ruling, in its own fixture. One WR starter is Out and
plays Thursday night, and every bench WR plays on Sunday. Yahoo locks him
into the slot at his kickoff, so that slot's deadline is capped at Thursday.
Taking the earliest replacement alone would say Sunday, three days late.

## `golden-lineup.json` and `golden-discovered.json`

The exact envelopes `body.rs` builds from the two input files above, with the
lineup's deadlines decided as of Tue 2026-09-29 15:00Z. `tests/golden.rs`
compares them byte for byte. They are the contract #855's pane parser and the
Settings line for discovered leagues are written against. **Regenerating them
to make a test pass defeats them.**
