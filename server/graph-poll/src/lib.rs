//! `hummingbird-graph-poll`: the out-of-process app-only-Graph
//! evaluated-stream pollers for `m365-mail/v1` and `m365-calendar/v1`
//! (#137, ADR-0011) — the third of #135-137, built directly onto
//! #135/#136's scaffolding (`server/gmail-poll`, `server/calendar-poll`).
//! Two binaries share this one lib: `graph-mail-poll`
//! (`src/bin/graph_mail_poll.rs`) and `graph-calendar-poll`
//! (`src/bin/graph_calendar_poll.rs`) — one crate rather than two, because
//! the entire auth leg (`auth.rs`) and the whole delta-cursor shape
//! (`delta.rs`, `resume.rs`) are identical between them; only the event
//! mapping, the parse, and each binary's own bound source differ.
//!
//! **Evaluate-in-poll**, both lanes: each run advances a `deltaLink`
//! cursor (`delta.rs`, `resume.rs`; shared, since Graph's delta protocol
//! uses one envelope shape for every resource collection — `delta.rs`'s
//! own module doc), folds the fetched items into candidates
//! (`mail_stream.rs`/`calendar_stream.rs` over
//! `mail_message.rs`/`calendar_item.rs`), and judges them **in memory**
//! against the live rule set (`evaluate.rs`, shared) before anything is
//! persisted — a non-match is dropped inside `evaluate_events` and never
//! reaches an HTTP call at all; only a match is upserted as an alert
//! (`alert.rs`, shared), through the exact same `POST /api/alerts` webhook
//! lane every other source uses.
//!
//! **A workspace member**, so CI's `cargo clippy --workspace` /
//! `cargo test --workspace` gate it. It must **never** become a dependency
//! of `hummingbird-authority-worker`: that crate builds for wasm32 and has
//! no business carrying an HTTP client, an RSA-signing dependency, or a
//! tzdb. Everything decidable — including the client-assertion signature
//! itself (`auth.rs`'s own doc: unlike `authority/src/fcm.rs`, this crate
//! is not wasm-constrained, so nothing about auth needs to live in the
//! untestable edge) — lives in the lib and is natively tested; each
//! binary's `main.rs` holds only `std::env`, the Graph OAuth HTTP POST,
//! and the Graph/authority HTTP calls.
//!
//! **Credential posture — decided on #486 Phase B, 2026-08-15.** ADR-0011's
//! original table places the M365 credential as a **Worker secret**; #135's
//! review round established that a GitHub Actions `schedule:` is a
//! different trust boundary, and #135/#136 both resolved it by narrowing
//! the credential to the least the poller's own calls need
//! (`gmail.readonly`, then reused for calendar). The certificate credential
//! here is narrow on both axes:
//!
//! - **In kind**: `Mail.Read`/`Calendars.Read`, both read-only application
//!   permissions, and neither binary in this crate ever calls a Graph write
//!   endpoint.
//! - **In reach**: an app-only Graph permission grants read **tenant-wide**
//!   by default — every mailbox and calendar the tenant has. An Exchange
//!   Online **Application Access Policy** now scopes the app registration
//!   to the single operator mailbox. `Test-ApplicationAccessPolicy`
//!   returned Granted for that mailbox and Denied for another in the same
//!   tenant; the operator ran both, and the private key entered Actions
//!   only afterwards.
//!
//! With reach bounded that way the credential sits on the
//! `CITY_WASTE_INGEST_TOKEN` side of CLAUDE.md's blast-radius line, not the
//! `ADMIN_SECRET`/`FCM_SERVICE_ACCOUNT` side. **The classification is
//! unchanged but the placement is not**: #774 moved both binaries off
//! GitHub Actions `schedule:` onto `hummingbird-sweeper`'s own supercronic
//! clock, so `GRAPH_CLIENT_PRIVATE_KEY` is now a **Fly secret** on that
//! machine (`flyctl secrets set`, from the operator's terminal — never
//! Actions) rather than an Actions secret, and the four identifiers above
//! are `fly.toml` `[env]` literals instead of `vars.*`. Rotation is now a
//! two-place operation (1Password → `flyctl secrets set`), the same shape
//! `docs/sweeper.md` already records for the `GOOGLE_*` three, once the
//! Actions copy of the key is deleted per #774's own checklist.
//! **The policy is the load-bearing half and nothing in this repo can
//! verify it**: if the app registration is ever re-created, or the policy
//! removed, the same key silently becomes a tenant-wide read again. The
//! verification record is on #486.
//!
//! **`original_start` — the recurring-occurrence key's second half — is
//! populated from Graph's own occurrence `id`, not a Google-style
//! `originalStartTime` field, because Graph's `event` resource has no such
//! field.** See `calendar_item.rs`'s module doc for the full reasoning;
//! this is the "case most likely to produce duplicate alerts if invented
//! locally" the issue's own brief calls out, and it is recorded here (and
//! posted to the issue) as an assumption unconfirmed against a live
//! tenant — `server/city-waste/src/page.rs`'s own "still open" precedent.
//!
//! **`occurred_at`/`starts_at`/`ends_at` are all UTC**, via
//! `hummingbird_domain::now_as_deadline` applied to each item's own
//! already-UTC millisecond timestamps (`mail_message.rs` reads Graph's
//! `receivedDateTime`, already UTC by construction; `calendar_item.rs`
//! reads `start`/`end.dateTime` requested with `Prefer:
//! outlook.timezone="UTC"` — its own module doc explains why that header
//! is load-bearing). `gmail_poll`'s own note that this does not generalize
//! to a source resolving day-shaped questions in a local zone applies here
//! too.

mod alert;
mod auth;
mod calendar_cursor;
mod calendar_event;
mod calendar_item;
mod calendar_stream;
mod delta;
mod evaluate;
mod mail_cursor;
mod mail_event;
mod mail_message;
mod mail_stream;
mod raw_id;
mod resume;

pub use alert::plan as plan_alert;
pub use auth::{
    client_assertion, parse_access_token, token_url, AccessToken, AuthError, CertCredential,
    CLIENT_ASSERTION_TYPE, GRAPH_SCOPE, TOKEN_EXPIRY_SLACK_SECS,
};
pub use calendar_cursor::{
    cursor_envelope as calendar_cursor_envelope, parse_cursor as parse_calendar_cursor,
    CursorError as CalendarCursorError, CURSOR_KEY as CALENDAR_CURSOR_KEY,
    POLLED_EVERY_MS as CALENDAR_POLLED_EVERY_MS, SOURCE as CALENDAR_SOURCE,
};
pub use calendar_event::calendar_item_to_candidate;
pub use calendar_item::{parse_calendar_item, CalendarItem, CalendarItemError, ParsedCalendarItem};
pub use calendar_stream::{fold_calendar_items, CalendarBatch};
pub use delta::{parse_delta_page, DeltaError, DeltaPage};
pub use evaluate::{evaluate_events, Candidate, Match};
pub use mail_cursor::{
    cursor_envelope as mail_cursor_envelope, parse_cursor as parse_mail_cursor, CursorError as MailCursorError,
    CURSOR_KEY as MAIL_CURSOR_KEY, POLLED_EVERY_MS as MAIL_POLLED_EVERY_MS, SOURCE as MAIL_SOURCE,
};
pub use mail_event::mail_message_to_candidate;
pub use mail_message::{parse_mail_message, MailMessage, MailMessageError, ParsedMailMessage};
pub use mail_stream::{fold_mail_messages, MailBatch};
pub use resume::{resume, DeltaOutcome, Plan};
