//! `hummingbird-gmail-poll`: the out-of-process `gmail/v1` evaluated-stream
//! poller (#135, ADR-0011) — the first of #135-137, and the one that
//! establishes the scaffolding the other two follow.
//!
//! **Evaluate-in-poll.** Each run advances a `historyId` delta cursor
//! (`cursor.rs`), fetches only the messages added since it (`history.rs`,
//! `message.rs`), maps each to an ADR-0013 `Event` (`event.rs`), and judges
//! the whole batch **in memory** against the live rule set
//! (`evaluate.rs`) before anything is persisted: a non-match is dropped
//! inside `evaluate_events` and never reaches an HTTP call at all. Only a
//! match is upserted as an alert (`alert.rs`), through the exact same
//! `POST /api/alerts` webhook lane every other source uses — this is what
//! keeps this poller from becoming a mail mirror, and it is the property
//! ADR-0011 names outright: "the authority holds only what rules promoted,
//! never the streams they were promoted from."
//!
//! **A workspace member**, so CI's `cargo clippy --workspace` /
//! `cargo test --workspace` gate it. It must **never** become a dependency
//! of `hummingbird-authority-worker`: that crate builds for wasm32 and has
//! no business carrying an HTTP client or an OAuth token exchange. The
//! split is the one `server/city-waste` and `authority/src/fcm.rs` /
//! `worker/src/fcm.rs` already draw: everything decidable lives here and is
//! natively tested against saved Gmail API response fixtures; `main.rs`
//! holds only `std::env`, the OAuth token exchange, and the Gmail/authority
//! HTTP calls — the untestable edge, kept as small as it can be.
//!
//! **The cursor's durable home** (a "key interface" the brief calls out
//! explicitly) is an ordinary `context_snapshots` row under this poller's
//! own bound source, `gmail/v1` (`cursor.rs`) — the exact lane
//! `server/city-waste` already writes through (`POST /api/snapshots`,
//! generic since #120), read back through the new `GET /api/snapshots`
//! this issue adds alongside it (`server/authority/src/handlers/snapshots.rs`),
//! since nothing before this poller ever needed to read its own snapshot
//! back. The rule set is read the same new way, through the new
//! `GET /api/rules` (`server/authority/src/handlers/rules.rs`) — an `ingest`
//! token could already write `alerts`/`snapshots` for its bound source;
//! reading the (uncredentialed, read-only) rule set every poller needs
//! doesn't widen what it can change.
//!
//! **A lost or invalid `historyId` recovers via a bounded re-sync**
//! (`main.rs`'s job — Gmail answers `history.list` with 404 once a
//! `historyId` has aged out, and only an HTTP status can see that): list
//! recent messages directly (`history::parse_messages_list`, bounded by
//! `main.rs`'s query and `maxResults`) and re-anchor the cursor at the
//! mailbox's current `historyId` (`history::parse_profile`). ADR-0011's own
//! words: "losing a cursor degrades to re-fetch-and-upsert, which the
//! dedupe key absorbs" — `alert::plan`'s never-sent `raised_at` and the
//! upsert's own no-op-on-identical-payload rule are what make that
//! re-fetch safe rather than merely tolerated.
//!
//! **The cursor-loss decision itself is `resume.rs`, not `main.rs`**
//! (#264 review item 5): `resume(stored, HistoryOutcome)` is a pure fold
//! over the stored cursor and the outcome of one `history.list` attempt —
//! `main.rs`'s only job is the `Ok`/`Status(404)` → `Page`/`Expired`
//! mapping, which needs the real HTTP status and so cannot move. AC6's
//! cursor-loss fixture case lives here, natively, rather than being
//! unreachable inside the untestable edge. `batch.rs`'s `fold_messages`
//! is its sibling for the per-message fetch loop (review item 4, as
//! redrawn by #685): a *transient* fetch failure aborts the whole batch
//! (`Err`, before `main.rs` ever calls `post_cursor`), while a permanent
//! one — a message Gmail no longer has, or a body that will not parse —
//! is skipped loudly but non-fatally. What sorts them is permanence, not
//! the layer the failure surfaces at, and drawing that line at
//! fetch-vs-parse instead is what wedged the live poller for six days on
//! one deleted message (#685): only a transient failure may be allowed
//! to block the cursor from advancing, because only a transient failure
//! stops being true.

//! **`occurred_at` and the evaluation clock are both `now_as_deadline(...)`,
//! i.e. UTC** — `event.rs` stamps a message's `occurred_at` from Gmail's own
//! `internalDate` through it, and `main.rs` reads "now" through it too, so
//! a rule's time predicates compare like-for-like within this poller. This
//! does not generalize to a source that resolves day-shaped questions in a
//! local zone (`server/city-waste`'s own `zoned-day.ts` carve-out) — it is
//! noted here only because that reasoning does not transfer.

mod alert;
mod batch;
mod cursor;
mod event;
mod evaluate;
mod history;
mod message;
mod resume;

pub use alert::plan as plan_alert;
pub use batch::{fold_messages, Batch};
pub use cursor::{envelope as cursor_envelope, parse_cursor, CursorError, CURSOR_KEY, CURSOR_SCHEMA, SOURCE};
pub use event::message_to_event;
pub use evaluate::{evaluate_events, Match};
pub use history::{
    parse_history_list, parse_messages_list, parse_profile, HistoryError, HistoryPage,
};
pub use message::{parse_message, GmailMessage, MessageError};
pub use resume::{resume, HistoryOutcome, Plan};
