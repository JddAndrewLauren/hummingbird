//! The frozen source registry (ADR-0014, widened by ADR-0014's 2026-08-11
//! amendment, #254). One entry per `source` string that can appear on
//! **any** of the three provenance-carrying tables ADR-0009 rule 4 gives
//! one frozen namespace convention to — `alerts`, `context_snapshots`, or
//! both — because a source's binding at mint time (#145) must be checked
//! against the same registry no matter which table it is destined to
//! write. `alerts.rs` mints an alert id as `sha256(source, source_key)` for
//! every alert-writing entry, so the `source_key` recipe *is* occurrence
//! identity there: changing it silently orphans every row already minted,
//! or silently absorbs new occurrences into stale ones. Neither symptom
//! rings — this registry is `sweep.py`'s frozen-namespace lesson re-learned
//! for the owned-server tables.
//!
//! **Each entry declares [`Writes`]** — which of `alerts`/`context_snapshots`
//! it may write. This is the second reader of the declaration, alongside
//! the mint gate: `POST /api/alerts` rejects a source [`SourceEntry`] finds
//! but does not declare [`Writes::writes_alerts`] for, and `POST
//! /api/snapshots` its `writes_snapshots` mirror. A source may of course be
//! *both* ([`Writes::Both`]): `city-waste/v2` writes a `context_snapshots`
//! row every poll **and** mints an alert on a holiday week, under one
//! string, because ADR-0009's join constraint is that the snapshot's
//! `source` and the alert's `source` are the same value. So do every
//! evaluated-stream poller's sources (`gmail/v1`, `m365-mail/v1`,
//! `google-calendar/v1`, `m365-calendar/v1`): each mints alerts on a rule
//! match **and** writes its own delta cursor (and, for
//! `google-calendar/v1`, the `busy_now` gauge too) as a `context_snapshots`
//! row under the same string.
//!
//! **`key_recipe` exists only where [`Writes::writes_alerts`] is true** —
//! it documents an alert's `source_key` recipe, which a snapshot-only entry
//! simply has none of (a snapshot's identity is `(source, key)`, authored
//! by the poller directly, never derived through a recipe function here).
//! The frozen-recipe tripwire below loses nothing from this: it keys off
//! the recipe *functions'* own frozen test vectors, not off this field,
//! which stays pure documentation.
//!
//! **Enrollment is at wiring time, not speculative.** ADR-0009 named three
//! sources this registry did not yet carry; the races lane is now enrolled
//! (`race-schedule/v1`, #266 — and as `Writes::Both`, not the snapshot-only
//! entry the ADR anticipated, because the same lane's second binary raises
//! the race-start alert under the same string). `github-hummingbird/v1`
//! enrolled at #314, snapshot-only, one source for every scheduled workflow
//! (ADR-0017 decision 2). One remains unenrolled (`anthropic-usage/v1`), and
//! it enrolls when its own lane is built, exactly as every source above did.
//! `find` returning `None` for it today means "not enrolled yet," not "will
//! never write here."
//!
//! Nothing here validates a recipe at runtime: `source_key` is opaque to
//! the server by design (no delimiter grammar, no parsing — ADR-0009 rule
//! 4's "entire common core" stays intact). What this registry buys is a
//! **build-time** tripwire: each source's key function is pinned by a
//! frozen test vector below, so changing the recipe fails `cargo test`
//! rather than failing silently in production. **Adapters must compute
//! every `source_key` through one of the functions in this module — there
//! is no other sanctioned way to build one.**
//!
//! The only sanctioned change to a shipped recipe is retiring the source
//! and minting a new versioned one (`city-waste/v1` → `city-waste/v2`),
//! never editing a recipe in place. [`SourceEntry::retired_as`] is how a
//! retired source is represented, distinct from a source the registry has
//! simply never heard of ([`find`] returns `None` for the latter).

/// Whether a source reports the state of a thing, or a discrete event
/// (ADR-0014). This is a wiring-time declaration, not derivable from
/// anything else on the source — pushed sources divide too, since a
/// healthcheck reports state while a GitHub webhook is an occurrence.
///
/// - **State**: `source_key` names *the thing*; occurrence is carried by
///   the alert's lifecycle (it leaves live when the thing resolves).
/// - **Event**: `source_key` names *the occurrence*; rows never resolve,
///   they leave only by ack or `expires_at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    State,
    Event,
}

/// What a source's `expires_at` derives from, if anything (ADR-0014:
/// "`expires_at` is set only where the occurrence has a natural end the
/// source already knows"). Three shapes, not two — `IfProvided` is
/// distinct from `Never`: a `Some`-shaped "none unless…" reading would
/// make `is_some()` true for a source that in practice never expires most
/// of the time, which is not what either variant means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expiry {
    /// The row never carries an `expires_at`; it sits until acked (event
    /// sources) or resolved (state sources).
    Never,
    /// The source always sets `expires_at`, derived as described.
    Always(&'static str),
    /// The source sets `expires_at` only when the event payload itself
    /// carries one; absent that, the row never expires.
    IfProvided(&'static str),
}

/// Which of the two provenance-carrying tables a source may write
/// (ADR-0014's 2026-08-11 amendment, #254). A wiring-time declaration, like
/// [`Shape`] — nothing here is derivable from a `source` string alone, since
/// two sources sharing one string convention (`/vN`) can differ completely
/// in which tables their adapter actually touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writes {
    /// Mints alert rows only — every webhook source enrolled before #254,
    /// plus `item-threshold/v1`, the internal DO-alarm sweep source.
    Alerts,
    /// Writes `context_snapshots` rows only — `kimi-balance/v1` (#313) and
    /// `github-hummingbird/v1` (#314) are both this shape today: a gauge (or,
    /// for #314, one gauge per scheduled workflow) with nothing for an
    /// `alerts` row to ever mean. The shape exists so any snapshot-only
    /// adapter has somewhere to enroll without forcing a `key_recipe` it has
    /// no use for.
    Snapshots,
    /// Both — every evaluated-stream poller's source (a delta cursor, and
    /// for `google-calendar/v1` the `busy_now` gauge too, alongside the
    /// alerts it mints) and `city-waste/v2` (its daily poll body alongside
    /// its holiday-week alert).
    Both,
}

impl Writes {
    pub fn writes_alerts(&self) -> bool {
        matches!(self, Writes::Alerts | Writes::Both)
    }

    pub fn writes_snapshots(&self) -> bool {
        matches!(self, Writes::Snapshots | Writes::Both)
    }
}

/// One registered source: its frozen, versioned `source` string, its
/// declared [`Shape`], which tables it may [`Writes`] to, its `source_key`
/// recipe where alert-writing makes one meaningful (documentation —
/// nothing machine-checks it beyond the recipe function's own frozen test
/// vector), its [`Expiry`], and its retirement state.
#[derive(Debug, Clone, Copy)]
pub struct SourceEntry {
    /// The versioned frozen namespace, e.g. `"gmail/v1"`. Every source
    /// carries a `/vN` suffix from the start (ADR-0014) — there is no bare
    /// name.
    pub source: &'static str,
    pub shape: Shape,
    /// Which of `alerts`/`context_snapshots` this source may write —
    /// [`POST /api/alerts`](crate) and `POST /api/snapshots` each reject a
    /// source this entry does not declare for their own table.
    pub writes: Writes,
    /// The `source_key` recipe, as documentation. `None` when
    /// `writes.writes_alerts()` is false — a snapshot-only source mints no
    /// `source_key` and so has no recipe to document. Not machine-checked
    /// here regardless; the corresponding `*_key` function's frozen test
    /// vector is what actually catches drift.
    pub key_recipe: Option<&'static str>,
    pub expires_at: Expiry,
    /// `Some(successor)` marks this source retired in favor of a newer
    /// version (e.g. `"city-waste/v2"`). Old rows under a retired source
    /// age out through their normal lifecycle; nothing new should be
    /// minted under it. `None` means the source is live.
    pub retired_as: Option<&'static str>,
}

impl SourceEntry {
    pub fn is_retired(&self) -> bool {
        self.retired_as.is_some()
    }

    pub fn writes_alerts(&self) -> bool {
        self.writes.writes_alerts()
    }

    pub fn writes_snapshots(&self) -> bool {
        self.writes.writes_snapshots()
    }
}

/// `item-threshold/v1`'s frozen namespace, named as a `const` (not just a
/// literal in the registry array below) so #138's DO alarm sweep — the one
/// caller that mints under this source without going through a webhook —
/// references this exact string rather than an independent copy. A
/// hand-typed second literal in `authority`'s sweep would drift silently
/// past the registry's own retirement tripwire the moment this source is
/// ever retired to `/v2`: `find` would still resolve the old string
/// (nothing would 400 or warn, since nothing calls `find` on a hardcoded
/// mint the way `validate_rule` does for a rule condition), and the sweep
/// would keep minting under a source the registry already flags as dead.
/// One `const`, two consumers, makes that drift a compile error instead.
pub const ITEM_THRESHOLD_V1: &str = "item-threshold/v1";

/// `city-waste/v2`'s frozen namespace, named for [`ITEM_THRESHOLD_V1`]'s
/// reason: two consumers share one literal — the registry entry below and
/// `server/city-waste`, the out-of-process poller that actually mints under
/// it — so a future retirement to `/v3` is a compile error at the poller
/// rather than a source string that quietly keeps resolving.
pub const CITY_WASTE_V2: &str = "city-waste/v2";

/// `gmail/v1`'s frozen namespace, named for [`CITY_WASTE_V2`]'s reason: two
/// consumers share one literal — the registry entry below and
/// `server/gmail-poll` (#135), the out-of-process poller that mints alerts
/// under it and also writes/reads its own delta cursor under it as a
/// `context_snapshots` row (ADR-0011: "each stream keeps a per-source delta
/// cursor") — so a future retirement to `/v2` is a compile error at the
/// poller rather than a source string that quietly keeps resolving.
pub const GMAIL_V1: &str = "gmail/v1";

/// `m365-mail/v1`'s frozen namespace, named for [`GMAIL_V1`]'s reason: two
/// consumers share one literal — the registry entry below and
/// `server/graph-poll`'s `graph-mail-poll` binary (#137), the out-of-process
/// poller that mints alerts under it and also writes/reads its own delta
/// cursor under it as a `context_snapshots` row (ADR-0011: "each stream
/// keeps a per-source delta cursor") — so a future retirement to `/v2` is a
/// compile error at the poller rather than a source string that quietly
/// keeps resolving.
pub const M365_MAIL_V1: &str = "m365-mail/v1";

/// `google-calendar/v1`'s frozen namespace, for [`GMAIL_V1`]'s own reason:
/// two consumers share one literal — the registry entry above and
/// `server/calendar-poll` (#136), the out-of-process poller that mints
/// alerts under it and also writes/reads both its delta sync-token cursor
/// and the `busy_now` snapshot as `context_snapshots` rows under it
/// (ADR-0011's "per-source delta cursor", and this issue's "a source may of
/// course be both" — an alert source and a snapshot source at once, exactly
/// like `city-waste/v2`) — so a future retirement to `/v2` is a compile
/// error at the poller rather than a source string that quietly keeps
/// resolving.
pub const GOOGLE_CALENDAR_V1: &str = "google-calendar/v1";

/// `m365-calendar/v1`'s frozen namespace, named for [`M365_MAIL_V1`]'s
/// reason: two consumers share one literal — the registry entry below and
/// `server/graph-poll`'s `graph-calendar-poll` binary (#137), which mints
/// alerts under it and writes/reads its own delta cursor under it as a
/// `context_snapshots` row.
pub const M365_CALENDAR_V1: &str = "m365-calendar/v1";

/// `race-schedule/v1`'s frozen namespace, named for [`CITY_WASTE_V2`]'s
/// reason: two consumers share one literal — the registry entry below and
/// `server/race-poll` (#266), whose `race-schedule-poll` binary writes one
/// `context_snapshots` row per adapted series under it and whose
/// `race-alert-poll` binary mints the race-start alert under the same
/// string — so a future retirement to `/v2` is a compile error at the
/// poller rather than a source string that quietly keeps resolving.
pub const RACE_SCHEDULE_V1: &str = "race-schedule/v1";

/// `kimi-balance/v1`'s frozen namespace (#313, ADR-0017 decision 5): two
/// consumers share one literal — the registry entry below and
/// `server/kimi-balance`, the out-of-process poller that mints the one
/// `context_snapshots` row under it every six hours — so a future retirement
/// to `/v2` (e.g. once Moonshot ships a consumption endpoint) is a compile
/// error at the poller rather than a source string that quietly keeps
/// resolving. **Snapshots only**: there is no spend endpoint to alert on, so
/// this source never mints an `alerts` row and carries no `key_recipe` — a
/// snapshot's identity is `(source, key)`, authored by the poller directly.
pub const KIMI_BALANCE_V1: &str = "kimi-balance/v1";

/// `github-hummingbird/v1`'s frozen namespace (#314, ADR-0017 decision 2):
/// two consumers share one literal — the registry entry below and
/// `server/github-status`, the out-of-process poller that mints one
/// `context_snapshots` row per scheduled workflow under it, keyed by the
/// workflow's file name — so a future retirement to `/v2` is a compile
/// error at the poller rather than a source string that quietly keeps
/// resolving. **One source for every GitHub fact, not one per workflow**:
/// the slash is already claimed by the version suffix, and folding a
/// sub-scope (which workflow) into a second source string per workflow is
/// exactly the unbounded, runtime-generated vocabulary ADR-0017 decision 2
/// rejects for the same reason it rejects one source per platform for
/// `race-schedule/v1`. **Snapshots only**: a scheduled workflow's run
/// history has nothing for an `alerts` row to mean — the pane derives its
/// own band from the snapshot at read time — so this source never mints an
/// `alerts` row and carries no `key_recipe`.
pub const GITHUB_HUMMINGBIRD_V1: &str = "github-hummingbird/v1";

/// The frozen registry. Every entry's `source` carries a version suffix
/// (enforced by `tests::every_registered_source_is_versioned`); every
/// source below has at least one frozen key-vector test in this module,
/// and the whole table is pinned verbatim by
/// `tests::registry_matches_the_frozen_adr_0014_table`.
pub const REGISTRY: &[SourceEntry] = &[
    SourceEntry {
        source: GMAIL_V1,
        shape: Shape::Event,
        // Both: mints an alert on a rule match, and separately writes/reads
        // its own delta cursor as a context_snapshots row (ADR-0011).
        writes: Writes::Both,
        key_recipe: Some("the Gmail message id"),
        expires_at: Expiry::Never,
        retired_as: None,
    },
    SourceEntry {
        source: M365_MAIL_V1,
        shape: Shape::Event,
        writes: Writes::Both,
        key_recipe: Some(
            "the mail's internetMessageId — never the Graph `id`, \
             which changes on a folder move",
        ),
        expires_at: Expiry::Never,
        retired_as: None,
    },
    SourceEntry {
        source: GOOGLE_CALENDAR_V1,
        shape: Shape::Event,
        // Both: mints an alert on a rule match, and writes both its delta
        // sync-token cursor and the busy_now gauge as context_snapshots
        // rows under the same string (two distinct `key`s, one source).
        writes: Writes::Both,
        key_recipe: Some("<eventId or recurringEventId>:<originalStartTime>"),
        expires_at: Expiry::Always("the instance's end time"),
        retired_as: None,
    },
    SourceEntry {
        source: M365_CALENDAR_V1,
        shape: Shape::Event,
        writes: Writes::Both,
        key_recipe: Some("<seriesMasterId or id>:<originalStart>"),
        expires_at: Expiry::Always("the instance's end time"),
        retired_as: None,
    },
    // Retired, deliberately, to give ADR-0014's second registry job (a
    // rule or an ingest token naming a retired source is flagged, never
    // silently inert) something real to exercise end-to-end — this is the
    // ADR's own worked example verbatim ("`source eq 'city-waste/v1'`
    // matching nothing forever after a bump", ADR-0014). Safe to retire
    // for real: nothing is deployed yet and the city-waste poller
    // (#135-137) is unbuilt, so no adapter has ever minted a row under
    // it. `city_waste_v1_key` stays defined and tested below regardless —
    // retirement never breaks the recipe old rows still need.
    //
    // `city-waste/v2` is registered directly below (#120), so `v1` is now
    // the ordinary retired case: it 400s at mint with its successor named,
    // and `v2` mints. Do NOT edit `v1`'s strings — a retired entry is
    // frozen, including the `key_recipe` and `expires_at` wording that
    // describe what its already-minted rows meant. `v1` predates the
    // city-waste poller entirely, so it was only ever an alert source —
    // `Writes::Alerts`, never `Both`.
    SourceEntry {
        source: "city-waste/v1",
        shape: Shape::Event,
        writes: Writes::Alerts,
        key_recipe: Some(
            "<stream>:<scheduled-date> — the originally scheduled \
             collection date, never whatever date a later \
             correction slides to",
        ),
        expires_at: Expiry::Always("end of the affected collection date"),
        retired_as: Some(CITY_WASTE_V2),
    },
    SourceEntry {
        source: CITY_WASTE_V2,
        shape: Shape::Event,
        // Both, from #254's amendment: the daily poll body every run
        // (context_snapshots) and a holiday-week alert (alerts) — #120's
        // "a source may of course be both," now a declared fact instead of
        // an unenrolled gap (absorbing #245's open note).
        writes: Writes::Both,
        key_recipe: Some(
            "the originally scheduled collection date, alone — never \
             the date a correction slides to, and never qualified \
             by which bins go out",
        ),
        expires_at: Expiry::Always(
            "end of the LATER of the scheduled and the slid-to collection date",
        ),
        retired_as: None,
    },
    SourceEntry {
        source: RACE_SCHEDULE_V1,
        // Event: a race start is a discrete occurrence, not the state of a
        // thing — `source_key` names the start instant, and the row leaves
        // live by ack or expiry rather than by resolving.
        shape: Shape::Event,
        // Both (#266): one context_snapshots row per adapted series, every
        // six hours, and the race-start alert at the lead time — one string,
        // two binaries, ADR-0009's join constraint intact.
        writes: Writes::Both,
        key_recipe: Some(
            "<series>:<race start instant, epoch ms> — the START INSTANT, \
             never season:round: a postponement must mint a NEW occurrence \
             so it rings, and the tidier season:round key silently would \
             not (no source-owned field changes, so nothing restamps)",
        ),
        expires_at: Expiry::Always("the race's start time"),
        retired_as: None,
    },
    // The registry's first live Snapshots-only entry (#313) — the case
    // `a_snapshot_only_entry_is_representable_and_not_alert_writing` below
    // exercised on a locally-built fixture before any real source needed it.
    // Moonshot's balance endpoint reports remaining balance, never
    // consumption (ADR-0017 decision 5), so there is nothing here for an
    // `alerts` row to ever mean.
    SourceEntry {
        source: KIMI_BALANCE_V1,
        shape: Shape::State,
        writes: Writes::Snapshots,
        key_recipe: None,
        expires_at: Expiry::Never,
        retired_as: None,
    },
    // The registry's second Snapshots-only entry (#314) — one row per
    // scheduled workflow, keyed by the workflow's file name, under this one
    // source string (ADR-0017 decision 2's "many panes, one source, one
    // question").
    SourceEntry {
        source: GITHUB_HUMMINGBIRD_V1,
        shape: Shape::State,
        writes: Writes::Snapshots,
        key_recipe: None,
        expires_at: Expiry::Never,
        retired_as: None,
    },
    SourceEntry {
        source: ITEM_THRESHOLD_V1,
        shape: Shape::State,
        // Alerts only: the internal DO-alarm sweep mints alerts against
        // items it reads directly — it writes no context_snapshots row.
        writes: Writes::Alerts,
        key_recipe: Some("item:<id>"),
        expires_at: Expiry::Never,
        retired_as: None,
    },
    SourceEntry {
        source: "healthchecks/v1",
        shape: Shape::State,
        writes: Writes::Alerts,
        key_recipe: Some("the check id, authored in the webhook body"),
        expires_at: Expiry::Never,
        retired_as: None,
    },
    SourceEntry {
        source: "home-assistant/v1",
        shape: Shape::State,
        writes: Writes::Alerts,
        key_recipe: Some("the entity id, authored in the webhook body"),
        expires_at: Expiry::Never,
        retired_as: None,
    },
    SourceEntry {
        source: "github/v1",
        shape: Shape::Event,
        writes: Writes::Alerts,
        key_recipe: Some("the source's own event id"),
        expires_at: Expiry::IfProvided("an expiry carried in the event payload, if any"),
        retired_as: None,
    },
    SourceEntry {
        source: "photo-site/v1",
        shape: Shape::Event,
        writes: Writes::Alerts,
        key_recipe: Some("the source's own event id"),
        expires_at: Expiry::IfProvided("an expiry carried in the event payload, if any"),
        retired_as: None,
    },
    SourceEntry {
        source: "gmail-alert/v1",
        shape: Shape::Event,
        writes: Writes::Alerts,
        key_recipe: Some("the source's own message id"),
        expires_at: Expiry::Never,
        retired_as: None,
    },
];

/// Looks a source string up in the registry. `None` means the registry has
/// never heard of this source — distinct from `Some(entry)` where
/// [`SourceEntry::is_retired`] is true, which means the registry knows it
/// and knows it is retired.
pub fn find(source: &str) -> Option<&'static SourceEntry> {
    REGISTRY.iter().find(|e| e.source == source)
}

// --- source_key recipes -----------------------------------------------
//
// Each function below is the ONLY sanctioned way to build the `source_key`
// for its source. Every one is a pure function of its arguments — no
// `now()`, no cursor, no row id — per ADR-0014's rule that an occurrence
// key must be re-derivable from the same occurrence tomorrow.

/// `gmail/v1`: the Gmail message id, verbatim.
pub fn gmail_v1_key(message_id: &str) -> String {
    message_id.to_string()
}

/// `m365-mail/v1`: `internetMessageId`, never the Graph `id`. The Graph
/// `id` is not even a parameter here — a folder move changes it, but the
/// key must not.
pub fn m365_mail_v1_key(internet_message_id: &str) -> String {
    internet_message_id.to_string()
}

/// `google-calendar/v1`: `<eventId or recurringEventId>:<originalStartTime>`.
/// `recurring_event_id` is `None` for a non-recurring event, in which case
/// `event_id` itself is the series identity.
pub fn google_calendar_v1_key(
    event_id: &str,
    recurring_event_id: Option<&str>,
    original_start_time: &str,
) -> String {
    format!(
        "{}:{}",
        recurring_event_id.unwrap_or(event_id),
        original_start_time
    )
}

/// `m365-calendar/v1`: `<seriesMasterId or id>:<originalStart>`.
pub fn m365_calendar_v1_key(
    id: &str,
    series_master_id: Option<&str>,
    original_start: &str,
) -> String {
    format!("{}:{}", series_master_id.unwrap_or(id), original_start)
}

/// `city-waste/v1`: `<stream>:<scheduled-date>`. `scheduled_date` must be
/// the *originally scheduled* collection date — the fixed coordinate — not
/// whatever date a later correction slides the pickup to; that is why this
/// function takes only one date, with no parameter for a corrected value
/// to leak in through.
pub fn city_waste_v1_key(stream: &str, scheduled_date: &str) -> String {
    format!("{stream}:{scheduled_date}")
}

/// `city-waste/v2`: the originally scheduled collection date, and nothing
/// else.
///
/// **Two parameters left v1, and each absence is the point.**
///
/// The slid-to date is absent for v1's reason, carried forward verbatim:
/// `scheduled_date` is the fixed coordinate, so a Tue → Wed correction to
/// one holiday lands on the row already minted for it rather than minting a
/// second alert about the same week.
///
/// The **stream** is absent because the domain corrected under it: there is
/// one collection day, and everything going out that week goes out together
/// (the finding that superseded the `waste-cadence` prototype's per-stream
/// fan-out). One week's slide is therefore one occurrence. Keying per
/// stream would mint three near-identical rows for one holiday, each
/// wanting its own dismissal. Which bins go out is a *property* of the
/// occurrence — carried in the alert's body and in the snapshot the pane
/// reads — never part of its identity.
pub fn city_waste_v2_key(scheduled_date: &str) -> String {
    scheduled_date.to_string()
}

/// `race-schedule/v1`: `<series>:<race start instant in epoch ms>`.
///
/// **The start instant, and deliberately not `season:round`.** `season:round`
/// is the tidier identity and it is what the feed hands over — and it fails
/// to ring on a postponement. The alert row would already exist under that
/// key; the title is clock-free by design (`race_poll::alert::plan` takes no
/// clock, for `city-waste`'s own reason), so no source-owned field changes,
/// so `restamp_on_change` does not restamp, and a race moved by a month
/// never re-raises. Keying on the start instant makes a rescheduled race a
/// *new* occurrence, which rings — correct for an [`Shape::Event`] source,
/// where `source_key` names the occurrence and not the thing. It also
/// self-cleans: `expires_at = starts_at_ms` means the occurrence left behind
/// by a reschedule expires at its own, now past, start time.
///
/// Two consequences. Keying on the instant is what lets `season` and `round`
/// stay **out** of the stored body — a `season:round` key would have forced
/// them back in, since `race-alert-poll` reads its material from the
/// snapshot. And a same-day *time correction* of a few minutes mints a new
/// occurrence and rings a second time; F1 start times rarely move once
/// published, and a start that did move is arguably worth a second ring.
pub fn race_schedule_v1_key(series: &str, starts_at_ms: i64) -> String {
    format!("{series}:{starts_at_ms}")
}

/// `item-threshold/v1`: `item:<id>`. Keyed on the item, not
/// `item:<id>:<deadline>` — a re-committed deadline must re-raise the same
/// row, never mint a second (ADR-0014).
pub fn item_threshold_v1_key(item_id: &str) -> String {
    format!("item:{item_id}")
}

/// `healthchecks/v1`: the check id, authored in the webhook body.
pub fn healthchecks_v1_key(check_id: &str) -> String {
    check_id.to_string()
}

/// `home-assistant/v1`: the entity id, authored in the webhook body.
pub fn home_assistant_v1_key(entity_id: &str) -> String {
    entity_id.to_string()
}

/// `github/v1`: the source's own event id.
pub fn github_v1_key(event_id: &str) -> String {
    event_id.to_string()
}

/// `photo-site/v1`: the source's own event id.
pub fn photo_site_v1_key(event_id: &str) -> String {
    event_id.to_string()
}

/// `gmail-alert/v1`: the source's own message id.
pub fn gmail_alert_v1_key(message_id: &str) -> String {
    message_id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Acceptance: every registered source has a versioned string.
    #[test]
    fn every_registered_source_is_versioned() {
        for entry in REGISTRY {
            let has_version_suffix = entry.source.rsplit_once("/v").is_some_and(|(_, suffix)| {
                !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
            });
            assert!(has_version_suffix, "{} has no /vN suffix", entry.source);
        }
    }

    /// Frozen table-pin: every entry's `(source, shape, writes, expires_at,
    /// retired_as)` tuple, verbatim, in registration order. `source` and
    /// `shape` are half of the `sha256(source, source_key)` alert id and
    /// drive the occurrence-lifecycle mechanism
    /// (`server/authority/src/handlers/alerts.rs`) — this is what actually
    /// catches `"gmail/v1"` silently drifting to `"gmail/v2"`, or
    /// `item-threshold/v1` silently flipping to `Shape::Event`, neither of
    /// which the version-suffix check or a single spot-checked entry would
    /// notice. `writes` is #254's own addition to the pin — the same drift
    /// class applies to a source silently losing (or gaining) a table
    /// declaration, since that is exactly what `POST /api/alerts` and
    /// `POST /api/snapshots` gate on.
    #[test]
    fn registry_matches_the_frozen_adr_0014_table() {
        let expected: &[(&str, Shape, Writes, Expiry, Option<&str>)] = &[
            ("gmail/v1", Shape::Event, Writes::Both, Expiry::Never, None),
            ("m365-mail/v1", Shape::Event, Writes::Both, Expiry::Never, None),
            (
                "google-calendar/v1",
                Shape::Event,
                Writes::Both,
                Expiry::Always("the instance's end time"),
                None,
            ),
            (
                "m365-calendar/v1",
                Shape::Event,
                Writes::Both,
                Expiry::Always("the instance's end time"),
                None,
            ),
            (
                "city-waste/v1",
                Shape::Event,
                Writes::Alerts,
                Expiry::Always("end of the affected collection date"),
                Some("city-waste/v2"),
            ),
            (
                "city-waste/v2",
                Shape::Event,
                Writes::Both,
                Expiry::Always(
                    "end of the LATER of the scheduled and the slid-to collection date",
                ),
                None,
            ),
            (
                "race-schedule/v1",
                Shape::Event,
                Writes::Both,
                Expiry::Always("the race's start time"),
                None,
            ),
            ("kimi-balance/v1", Shape::State, Writes::Snapshots, Expiry::Never, None),
            ("github-hummingbird/v1", Shape::State, Writes::Snapshots, Expiry::Never, None),
            ("item-threshold/v1", Shape::State, Writes::Alerts, Expiry::Never, None),
            ("healthchecks/v1", Shape::State, Writes::Alerts, Expiry::Never, None),
            ("home-assistant/v1", Shape::State, Writes::Alerts, Expiry::Never, None),
            (
                "github/v1",
                Shape::Event,
                Writes::Alerts,
                Expiry::IfProvided("an expiry carried in the event payload, if any"),
                None,
            ),
            (
                "photo-site/v1",
                Shape::Event,
                Writes::Alerts,
                Expiry::IfProvided("an expiry carried in the event payload, if any"),
                None,
            ),
            ("gmail-alert/v1", Shape::Event, Writes::Alerts, Expiry::Never, None),
        ];

        assert_eq!(
            REGISTRY.len(),
            expected.len(),
            "registry gained or lost a source"
        );
        for (entry, (source, shape, writes, expires_at, retired_as)) in REGISTRY.iter().zip(expected) {
            assert_eq!(entry.source, *source);
            assert_eq!(entry.shape, *shape, "{} shape drifted", entry.source);
            assert_eq!(entry.writes, *writes, "{} writes drifted", entry.source);
            assert_eq!(
                entry.expires_at, *expires_at,
                "{} expires_at drifted",
                entry.source
            );
            assert_eq!(
                entry.retired_as, *retired_as,
                "{} retirement drifted",
                entry.source
            );
        }
    }

    /// Every alert-writing entry documents a recipe; every entry that is
    /// NOT alert-writing documents none — `key_recipe` and
    /// `writes_alerts()` never disagree, or the field is decoration for one
    /// direction and a silent gap for the other.
    #[test]
    fn key_recipe_is_present_iff_the_entry_writes_alerts() {
        for entry in REGISTRY {
            assert_eq!(
                entry.key_recipe.is_some(),
                entry.writes_alerts(),
                "{}: key_recipe presence must match writes_alerts()",
                entry.source
            );
        }
    }

    /// Acceptance: `Writes` answers `writes_alerts`/`writes_snapshots`
    /// correctly for all three states, including the `Snapshots`-only case
    /// — which `kimi-balance/v1` (#313) and `github-hummingbird/v1` (#314)
    /// now both exercise for real — proven directly on the enum so a
    /// regression is caught here even if a real entry's own pinned test
    /// were somehow weakened.
    #[test]
    fn writes_predicates_cover_all_three_states() {
        assert!(Writes::Alerts.writes_alerts());
        assert!(!Writes::Alerts.writes_snapshots());

        assert!(!Writes::Snapshots.writes_alerts());
        assert!(Writes::Snapshots.writes_snapshots());

        assert!(Writes::Both.writes_alerts());
        assert!(Writes::Both.writes_snapshots());
    }

    /// A locally built snapshot-only entry — the case ADR-0009's
    /// `anthropic-usage/v1` will eventually be (the third lane it named,
    /// races, enrolled at #266 as `Writes::Both` instead, since its second
    /// binary also raises an alert) — is representable and correctly
    /// rejected for the alerts table, exactly like
    /// [`a_retired_source_is_representable_and_distinct_from_unknown`]
    /// exercises retirement without a second real retired entry.
    #[test]
    fn a_snapshot_only_entry_is_representable_and_not_alert_writing() {
        let entry = SourceEntry {
            source: "anthropic-usage/v1",
            shape: Shape::State,
            writes: Writes::Snapshots,
            key_recipe: None,
            expires_at: Expiry::Never,
            retired_as: None,
        };
        assert!(!entry.writes_alerts(), "a snapshot-only entry must not declare alerts");
        assert!(entry.writes_snapshots());
        assert!(entry.key_recipe.is_none(), "no recipe to document");
    }

    /// `city-waste/v2` is the one live example of `Writes::Both` reachable
    /// through the real registry (#254 amends it in) — pinned directly so a
    /// regression to `Alerts`-only silently re-breaks #120's snapshot lane.
    #[test]
    fn city_waste_v2_is_registered_for_both_tables() {
        let entry = find(CITY_WASTE_V2).expect("city-waste/v2 is registered");
        assert!(entry.writes_alerts());
        assert!(entry.writes_snapshots());
    }

    /// `race-schedule/v1` is the second live `Writes::Both` entry (#266) —
    /// the six-hourly season snapshot per adapted series and the race-start
    /// alert, under one string and two binaries.
    #[test]
    fn race_schedule_v1_is_registered_for_both_tables() {
        let entry = find(RACE_SCHEDULE_V1).expect("race-schedule/v1 is registered");
        assert!(entry.writes_alerts());
        assert!(entry.writes_snapshots());
        assert_eq!(entry.shape, Shape::Event, "a race start is an occurrence");
        assert_eq!(entry.expires_at, Expiry::Always("the race's start time"));
    }

    /// `kimi-balance/v1` (#313) is the registry's first real, live
    /// Snapshots-only entry — pinned directly against `find` so a
    /// regression to `Alerts` or `Both` silently re-adds an `alerts` write
    /// path this source has no `source_key` recipe for.
    #[test]
    fn kimi_balance_v1_is_registered_snapshot_only() {
        let entry = find(KIMI_BALANCE_V1).expect("kimi-balance/v1 is registered");
        assert!(!entry.writes_alerts());
        assert!(entry.writes_snapshots());
        assert_eq!(entry.shape, Shape::State);
        assert!(entry.key_recipe.is_none(), "no recipe to document");
        assert!(!entry.is_retired());
    }

    /// `github-hummingbird/v1` (#314) is the registry's second live
    /// Snapshots-only entry — pinned directly against `find` so a regression
    /// to `Alerts` or `Both` silently re-adds an `alerts` write path this
    /// source has no `source_key` recipe for. One source, many panes (one
    /// per scheduled workflow, keyed by file name) — the registry itself
    /// carries no notion of "many panes"; that fan-out lives entirely in the
    /// poller and the pane.
    #[test]
    fn github_hummingbird_v1_is_registered_snapshot_only() {
        let entry = find(GITHUB_HUMMINGBIRD_V1).expect("github-hummingbird/v1 is registered");
        assert!(!entry.writes_alerts());
        assert!(entry.writes_snapshots());
        assert_eq!(entry.shape, Shape::State);
        assert!(entry.key_recipe.is_none(), "no recipe to document");
        assert!(!entry.is_retired());
    }

    /// `item-threshold/v1` is the one live example of an alerts-only entry
    /// that is NOT a webhook source — pinned so the internal DO-alarm sweep
    /// never silently gains a snapshot-writing declaration it has no writer
    /// for.
    #[test]
    fn item_threshold_v1_is_alerts_only() {
        let entry = find(ITEM_THRESHOLD_V1).expect("item-threshold/v1 is registered");
        assert!(entry.writes_alerts());
        assert!(!entry.writes_snapshots());
    }

    /// A duplicate `source` string would shadow silently in [`find`]
    /// (`iter().find` returns only the first match) — every source must be
    /// unique.
    #[test]
    fn registry_source_strings_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for entry in REGISTRY {
            assert!(
                seen.insert(entry.source),
                "duplicate source string: {}",
                entry.source
            );
        }
    }

    /// Acceptance: an unknown source is distinguishable from a retired one
    /// — `find` returns `None` for a string the registry has never heard
    /// of.
    #[test]
    fn find_returns_none_for_an_unregistered_source() {
        assert!(find("weather-alert/v1").is_none());
    }

    #[test]
    fn find_returns_the_entry_for_a_registered_source() {
        let entry = find("gmail/v1").expect("gmail/v1 is registered");
        assert_eq!(entry.shape, Shape::Event);
        assert!(!entry.is_retired());
    }

    /// `city-waste/v1` is the registry's one real retired entry (#189) —
    /// `find` resolves it, and it reports retired with its successor,
    /// through the actual production path (not a locally-built fixture).
    #[test]
    fn find_resolves_the_one_retired_registry_entry() {
        let entry = find("city-waste/v1").expect("city-waste/v1 is registered");
        assert!(entry.is_retired());
        assert_eq!(entry.retired_as, Some("city-waste/v2"));
    }

    /// Acceptance: a retired source is representable, and distinguishable
    /// from an unknown one — `Some(entry)` with `is_retired() == true`
    /// versus `find`'s `None` for a string never registered at all.
    /// `city-waste/v1` above is now genuinely retired (#189), but this
    /// test still exercises the mechanism on a locally built entry rather
    /// than on `REGISTRY` — it is about `SourceEntry`/`is_retired` in
    /// isolation; `find_resolves_the_one_retired_registry_entry` above is
    /// the one that goes through the real registry.
    #[test]
    fn a_retired_source_is_representable_and_distinct_from_unknown() {
        let retired = SourceEntry {
            source: "city-waste/v1",
            shape: Shape::Event,
            writes: Writes::Alerts,
            key_recipe: Some("<stream>:<scheduled-date>"),
            expires_at: Expiry::Always("end of the affected collection date"),
            retired_as: Some("city-waste/v2"),
        };
        assert!(retired.is_retired());
        assert_eq!(retired.retired_as, Some("city-waste/v2"));

        // Retirement never breaks the key recipe itself — old rows must
        // stay computable so their normal lifecycle (ack/expiry) can play
        // out.
        assert_eq!(city_waste_v1_key("trash", "2026-08-17"), "trash:2026-08-17");

        let unknown = find("city-waste/v0-does-not-exist");
        assert!(unknown.is_none());
    }

    // --- frozen key vectors ---------------------------------------------
    // One sample occurrence in, the expected `source_key` out, per source.
    // A change to a recipe that breaks one of these is exactly the failing
    // build ADR-0014 asks for.

    #[test]
    fn gmail_v1_keys_on_the_message_id() {
        assert_eq!(gmail_v1_key("18c2f9a0b1e2d3f4"), "18c2f9a0b1e2d3f4");
    }

    /// Pins that the key is `internetMessageId`, unaffected by a folder
    /// move. Modeled on two real Graph API observations of the *same*
    /// mail, before and after a move — the Graph `id` genuinely changes
    /// (verified 2026-08-09, ADR-0014) while `internetMessageId` does not.
    /// `m365_mail_v1_key` has no parameter for the Graph `id` at all, so
    /// this both demonstrates the move happened and that it cannot reach
    /// the key.
    #[test]
    fn m365_mail_v1_key_is_unchanged_by_a_folder_move() {
        struct GraphObservation {
            graph_id: &'static str,
            internet_message_id: &'static str,
        }
        let before_move = GraphObservation {
            graph_id: "AAMkAGI1AAA=-inbox",
            internet_message_id: "<abc123@contoso.com>",
        };
        let after_move = GraphObservation {
            // A real folder move: the Graph `id` is a different string
            // after the mail lands in Archive, but the sender-assigned
            // `internetMessageId` cannot change — Exchange preserves it.
            graph_id: "AAMkAGI1AAA=-archive",
            internet_message_id: "<abc123@contoso.com>",
        };
        assert_ne!(
            before_move.graph_id, after_move.graph_id,
            "the move must actually change the Graph id"
        );

        let key_before_move = m365_mail_v1_key(before_move.internet_message_id);
        let key_after_move = m365_mail_v1_key(after_move.internet_message_id);
        assert_eq!(key_before_move, "<abc123@contoso.com>");
        assert_eq!(key_before_move, key_after_move);
    }

    /// Pins that two occurrences of one recurring series get different
    /// keys, and the same occurrence seen twice gets the same key.
    #[test]
    fn google_calendar_v1_keys_recurring_instances_by_original_start_time() {
        let first = google_calendar_v1_key("inst-1", Some("series-abc"), "2026-08-10T09:00:00");
        let second = google_calendar_v1_key("inst-2", Some("series-abc"), "2026-08-17T09:00:00");
        assert_eq!(first, "series-abc:2026-08-10T09:00:00");
        assert_eq!(second, "series-abc:2026-08-17T09:00:00");
        assert_ne!(
            first, second,
            "two instances of one series must key differently"
        );

        // The same occurrence, re-fetched (e.g. after a lost cursor):
        let first_again =
            google_calendar_v1_key("inst-1", Some("series-abc"), "2026-08-10T09:00:00");
        assert_eq!(first, first_again);
    }

    #[test]
    fn google_calendar_v1_falls_back_to_event_id_for_a_non_recurring_event() {
        assert_eq!(
            google_calendar_v1_key("solo-event-1", None, "2026-08-10T09:00:00"),
            "solo-event-1:2026-08-10T09:00:00"
        );
    }

    #[test]
    fn m365_calendar_v1_keys_on_series_master_and_original_start() {
        assert_eq!(
            m365_calendar_v1_key("inst-1", Some("series-xyz"), "2026-08-10T09:00:00"),
            "series-xyz:2026-08-10T09:00:00"
        );
        assert_eq!(
            m365_calendar_v1_key("solo-event-1", None, "2026-08-10T09:00:00"),
            "solo-event-1:2026-08-10T09:00:00"
        );
    }

    /// Pins the clause that bites: a correction to the *slid-to* date must
    /// not remint the key. Modeled on two real polls of the same city page
    /// — `scheduled_date` (the fixed coordinate the key is built from)
    /// never moves, but `slides_to` (what the correction actually
    /// changes) genuinely does, Tuesday the 18th to Wednesday the 19th.
    #[test]
    fn city_waste_v1_keys_on_the_scheduled_date_not_a_later_correction() {
        struct CityPagePoll {
            scheduled_date: &'static str,
            /// The mutable value a correction changes; never passed to
            /// `city_waste_v1_key` — this is the value ADR-0014 says the
            /// key must ignore.
            slides_to: &'static str,
        }
        let first_poll = CityPagePoll {
            scheduled_date: "2026-08-17",
            slides_to: "2026-08-18",
        };
        let corrected_poll = CityPagePoll {
            scheduled_date: "2026-08-17",
            // The city corrects the slide from Tuesday the 18th to
            // Wednesday the 19th — a real change to the mutable value.
            slides_to: "2026-08-19",
        };
        assert_ne!(
            first_poll.slides_to, corrected_poll.slides_to,
            "the correction must actually move the slide date"
        );

        let key_when_first_seen = city_waste_v1_key("trash", first_poll.scheduled_date);
        let key_after_correction = city_waste_v1_key("trash", corrected_poll.scheduled_date);
        assert_eq!(key_when_first_seen, "trash:2026-08-17");
        assert_eq!(key_when_first_seen, key_after_correction);
    }

    /// v2's frozen vector, and it pins **two** independent ways the key
    /// must refuse to move — the pair that decides whether one holiday is
    /// one occurrence or several.
    ///
    /// A *correction* (Tue → Wed) is v1's clause carried forward: the
    /// slid-to date is not a parameter, so it cannot reach the key.
    ///
    /// A *change in which bins go out* is the new one. Under the corrected
    /// domain there is one collection day, so a week where recycling joins
    /// the trash is the same collection, not a second one — and the key
    /// takes no stream, so the two readings below are the same occurrence
    /// by construction. If this ever regresses to `<stream>:<date>`, one
    /// holiday starts minting a row per bin and each wants its own
    /// dismissal.
    #[test]
    fn city_waste_v2_keys_on_the_scheduled_date_alone() {
        struct CityPagePoll {
            scheduled_date: &'static str,
            /// Neither of these is a parameter of `city_waste_v2_key` —
            /// they are what the key must ignore.
            slides_to: &'static str,
            streams: &'static [&'static str],
        }
        let first_poll = CityPagePoll {
            scheduled_date: "2026-08-17",
            slides_to: "2026-08-18",
            streams: &["trash"],
        };
        let corrected_poll = CityPagePoll {
            scheduled_date: "2026-08-17",
            slides_to: "2026-08-19",
            streams: &["trash", "recycling"],
        };
        assert_ne!(
            first_poll.slides_to, corrected_poll.slides_to,
            "the correction must actually move the slide date"
        );
        assert_ne!(
            first_poll.streams, corrected_poll.streams,
            "the second reading must actually differ in which bins go out"
        );

        let key_when_first_seen = city_waste_v2_key(first_poll.scheduled_date);
        let key_after_correction = city_waste_v2_key(corrected_poll.scheduled_date);
        assert_eq!(key_when_first_seen, "2026-08-17");
        assert_eq!(key_when_first_seen, key_after_correction);
    }

    /// The next week's holiday is a *different* occurrence — ADR-0012's
    /// Christmas/New Year case, written about this very source. Pinned
    /// alongside the vector above, because "the key never moves" and "two
    /// weeks are two occurrences" are the two halves of one property and a
    /// key that over-collapsed would satisfy only the first.
    #[test]
    fn city_waste_v2_keys_two_consecutive_holiday_weeks_apart() {
        assert_ne!(
            city_waste_v2_key("2026-12-21"),
            city_waste_v2_key("2026-12-28")
        );
    }

    /// `race-schedule/v1`'s frozen vector: the series and the start instant,
    /// in epoch ms, and nothing else. The literal is the 2026 Australian
    /// Grand Prix's own start as the feed publishes it.
    #[test]
    fn race_schedule_v1_keys_on_the_series_and_the_start_instant() {
        assert_eq!(race_schedule_v1_key("f1", 1_772_942_400_000), "f1:1772942400000");
    }

    /// The property the recipe exists for, and the one `season:round` would
    /// have failed: a **postponed** race is a new occurrence, so it rings —
    /// while an unchanged re-poll of the same race lands on the same row for
    /// six months of twice-hourly polls, so it does not.
    #[test]
    fn race_schedule_v1_makes_a_postponement_a_new_occurrence() {
        struct RaceReading {
            /// Neither of these is a parameter — they are what a
            /// `season:round` key would have used, and what this one must
            /// ignore.
            season: &'static str,
            round: &'static str,
            starts_at_ms: i64,
        }
        let published = RaceReading { season: "2026", round: "1", starts_at_ms: 1_772_942_400_000 };
        let postponed = RaceReading {
            season: "2026",
            round: "1",
            starts_at_ms: 1_772_942_400_000 + 28 * 24 * 60 * 60 * 1000,
        };
        assert_eq!(
            (published.season, published.round),
            (postponed.season, postponed.round),
            "the feed calls a postponed race the same season and round"
        );
        assert_ne!(
            race_schedule_v1_key("f1", published.starts_at_ms),
            race_schedule_v1_key("f1", postponed.starts_at_ms),
            "a postponement must mint a new occurrence, or it never rings"
        );
        assert_eq!(
            race_schedule_v1_key("f1", published.starts_at_ms),
            race_schedule_v1_key("f1", published.starts_at_ms),
            "an unchanged re-poll lands on the row it already minted"
        );
    }

    /// Two series' races at the same instant are two occurrences — the
    /// series is in the key, not merely in the `subject_key`, so an IndyCar
    /// adapter landing later cannot overwrite an F1 row.
    #[test]
    fn race_schedule_v1_keys_two_series_apart_at_the_same_instant() {
        assert_ne!(
            race_schedule_v1_key("f1", 1_772_942_400_000),
            race_schedule_v1_key("indycar", 1_772_942_400_000)
        );
    }

    #[test]
    fn item_threshold_v1_keys_on_the_item_id_alone() {
        assert_eq!(item_threshold_v1_key("item-42"), "item:item-42");
    }

    #[test]
    fn healthchecks_v1_keys_on_the_check_id() {
        assert_eq!(healthchecks_v1_key("check-9"), "check-9");
    }

    #[test]
    fn home_assistant_v1_keys_on_the_entity_id() {
        assert_eq!(
            home_assistant_v1_key("binary_sensor.front_door"),
            "binary_sensor.front_door"
        );
    }

    #[test]
    fn github_v1_keys_on_the_event_id() {
        assert_eq!(github_v1_key("evt-123"), "evt-123");
    }

    #[test]
    fn photo_site_v1_keys_on_the_event_id() {
        assert_eq!(photo_site_v1_key("evt-456"), "evt-456");
    }

    #[test]
    fn gmail_alert_v1_keys_on_the_message_id() {
        assert_eq!(gmail_alert_v1_key("msg-789"), "msg-789");
    }
}
