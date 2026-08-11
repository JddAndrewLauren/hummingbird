//! `hummingbird-calendar-poll`: the out-of-process `google-calendar/v1`
//! evaluated-stream poller (#136, ADR-0011) — the second of #135-137,
//! built directly onto #135's scaffolding (`server/gmail-poll`).
//!
//! **Two jobs in one poll.** The evaluated stream: a sync-token cursor
//! (`cursor.rs`, `sync.rs`) over `events.list`, folded into evaluation
//! candidates (`stream.rs`, `event.rs`), judged **in memory** against the
//! live rule set (`evaluate.rs`) before anything is persisted — a
//! non-match is dropped inside `evaluate_events` and never reaches an HTTP
//! call at all; only a match is upserted as an alert (`alert.rs`), through
//! the exact same `POST /api/alerts` webhook lane every other source uses.
//! And the `calendar_busy` snapshot (`busy.rs`): a *separate*, always-run
//! query around "now" (never the sync-token cursor, which only ever
//! answers "what changed" — busy needs "what's true right now"), replaced
//! wholesale each poll as an ordinary `context_snapshots` row.
//!
//! **A workspace member**, so CI's `cargo clippy --workspace` /
//! `cargo test --workspace` gate it. It must **never** become a dependency
//! of `hummingbird-authority-worker`: that crate builds for wasm32 and has
//! no business carrying an HTTP client or an OAuth token exchange. The
//! split is `gmail_poll`'s own: everything decidable lives here and is
//! natively tested against saved Calendar API response fixtures; `main.rs`
//! holds only `std::env`, the OAuth token exchange, and the
//! Calendar/authority HTTP calls — the untestable edge, kept as small as
//! it can be.
//!
//! **The cursor and the busy gauge share one bound source**
//! (`cursor.rs::SOURCE`, `google-calendar/v1`) but different
//! `context_snapshots.key`s (`CURSOR_KEY` = `"cursor"`, `BUSY_KEY` =
//! `"busy_now"`) — `sources.rs`'s "a source may of course be both",
//! extended here to a source being three things under one string: an alert
//! source and two independent snapshot rows.
//!
//! **`google-calendar/v1`'s occurrence key follows #158's convention**
//! exactly (`hummingbird_domain::google_calendar_v1_key`):
//! `<eventId or recurringEventId>:<originalStartTime>`, which is what makes
//! a recurring event's *instances* distinct occurrences rather than one
//! alert overwritten on every recurrence, while a rescheduled instance
//! (Google issues a NEW `id` on some reschedules, but `originalStartTime`
//! is stable) still lands on the row minted for its original slot rather
//! than a fresh one.
//!
//! **`google-calendar/v1` is registered with
//! `Expiry::Always("the instance's end time")`**
//! (`hummingbird_domain::sources::REGISTRY`) — unlike `gmail/v1`, which
//! never expires. `alert.rs::plan` sets `expires_at` from
//! [`evaluate::Match::ends_at_ms`] on every alert this poller mints, which
//! is why [`evaluate::Candidate`]/[`evaluate::Match`] carry that field
//! through where `gmail_poll`'s own `Match` does not need to.
//!
//! **The cursor-loss decision is `resume.rs`, not `main.rs`**
//! (`gmail_poll::resume`'s own pattern, #264 review item 5): `resume(stored,
//! SyncOutcome)` is a pure fold over the stored `syncToken` and the outcome
//! of one `events.list` attempt — `main.rs`'s only job is the
//! `Ok`/`Status(410)` → `Page`/`Expired` mapping, which needs the real HTTP
//! status and so cannot move. There is no `batch.rs` analogue here for the
//! per-item fetch-failure fold: `events.list` already returns full event
//! bodies (unlike Gmail's `history.list`, ids only), so there is no
//! separate per-item network call to fail transiently — see `stream.rs`'s
//! module doc for why `fold_events` is pure rather than fallible.
//!
//! **`occurred_at`/`starts_at`/`ends_at` are all UTC**, via
//! `hummingbird_domain::now_as_deadline` applied to each event's own
//! `starts_at_ms`/`ends_at_ms` — `gmail_poll`'s own note that this does not
//! generalize to a source resolving day-shaped questions in a local zone
//! applies here too.

mod alert;
mod busy;
mod calendar_event;
mod cursor;
mod event;
mod evaluate;
mod resume;
mod stream;
mod sync;

pub use alert::plan as plan_alert;
pub use busy::{busy_window, live_calendar_events, BusyWindow};
pub use calendar_event::{parse_calendar_event, CalendarEvent, CalendarEventError, ParsedCalendarEvent};
pub use cursor::{
    busy_envelope, cursor_envelope, parse_cursor, CursorError, BUSY_KEY, BUSY_SCHEMA, CURSOR_KEY,
    CURSOR_SCHEMA, SOURCE,
};
pub use event::calendar_event_to_event;
pub use evaluate::{evaluate_events, Candidate, Match};
pub use resume::{resume, Plan, SyncOutcome};
pub use stream::{fold_events, Batch};
pub use sync::{parse_events_list, SyncError, SyncPage};
