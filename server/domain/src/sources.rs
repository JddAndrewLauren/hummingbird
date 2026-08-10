//! The frozen alert-source registry (ADR-0014). One entry per `source`
//! string that can appear on an **alert** — i.e. every source `alerts.rs`
//! mints ids for, which is the exact table given in the #158 brief and in
//! ADR-0014's conventions table. `alerts.rs` mints an alert id as
//! `sha256(source, source_key)`, so the `source_key` recipe *is* occurrence
//! identity: changing it silently orphans every row already minted, or
//! silently absorbs new occurrences into stale ones. Neither symptom rings
//! — this registry is `sweep.py`'s frozen-namespace lesson re-learned for
//! alerts.
//!
//! **Out of scope, deliberately:** `items.source` and
//! `context_snapshots.source` (e.g. `web/v1`, `anthropic-usage/v1`,
//! `github-hummingbird/v1`, `f1/v1`) are the same frozen namespace by
//! ADR-0009 rule 4 and must carry the same `/vN` version-suffix
//! convention, but they are not alert sources, mint no `source_key`
//! through `alerts.rs`, and are not enrolled here. `find` returning `None`
//! for one of them means "not an alert source," not "unversioned" or
//! "unknown to the codebase" — do not read absence from this registry as a
//! naming-convention violation on those tables.
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

/// One registered source: its frozen, versioned `source` string, its
/// declared [`Shape`], its `source_key` recipe (documentation — nothing
/// machine-checks it beyond the recipe function's own frozen test vector),
/// its [`Expiry`], and its retirement state.
#[derive(Debug, Clone, Copy)]
pub struct SourceEntry {
    /// The versioned frozen namespace, e.g. `"gmail/v1"`. Every source
    /// carries a `/vN` suffix from the start (ADR-0014) — there is no bare
    /// name.
    pub source: &'static str,
    pub shape: Shape,
    /// The `source_key` recipe, as documentation. Not machine-checked here;
    /// the corresponding `*_key` function's frozen test vector is what
    /// actually catches drift.
    pub key_recipe: &'static str,
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

/// The frozen registry. Every entry's `source` carries a version suffix
/// (enforced by `tests::every_registered_source_is_versioned`); every
/// source below has at least one frozen key-vector test in this module,
/// and the whole table is pinned verbatim by
/// `tests::registry_matches_the_frozen_adr_0014_table`.
pub const REGISTRY: &[SourceEntry] = &[
    SourceEntry {
        source: "gmail/v1",
        shape: Shape::Event,
        key_recipe: "the Gmail message id",
        expires_at: Expiry::Never,
        retired_as: None,
    },
    SourceEntry {
        source: "m365-mail/v1",
        shape: Shape::Event,
        key_recipe: "the mail's internetMessageId — never the Graph `id`, \
                      which changes on a folder move",
        expires_at: Expiry::Never,
        retired_as: None,
    },
    SourceEntry {
        source: "google-calendar/v1",
        shape: Shape::Event,
        key_recipe: "<eventId or recurringEventId>:<originalStartTime>",
        expires_at: Expiry::Always("the instance's end time"),
        retired_as: None,
    },
    SourceEntry {
        source: "m365-calendar/v1",
        shape: Shape::Event,
        key_recipe: "<seriesMasterId or id>:<originalStart>",
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
    // No `city-waste/v2` entry exists here yet — that lands with #135-137,
    // whichever poller actually produces `/v2` rows. Until then `city-waste`
    // is entirely unmintable as an ingest-token source (`v1` 400s as
    // retired, `v2` 400s as unregistered): correct per ADR-0014, and loud
    // rather than silent, but worth knowing going in rather than
    // rediscovering at that poller's `POST /api/admin/tokens` step.
    SourceEntry {
        source: "city-waste/v1",
        shape: Shape::Event,
        key_recipe: "<stream>:<scheduled-date> — the originally scheduled \
                      collection date, never whatever date a later \
                      correction slides to",
        expires_at: Expiry::Always("end of the affected collection date"),
        retired_as: Some("city-waste/v2"),
    },
    SourceEntry {
        source: ITEM_THRESHOLD_V1,
        shape: Shape::State,
        key_recipe: "item:<id>",
        expires_at: Expiry::Never,
        retired_as: None,
    },
    SourceEntry {
        source: "healthchecks/v1",
        shape: Shape::State,
        key_recipe: "the check id, authored in the webhook body",
        expires_at: Expiry::Never,
        retired_as: None,
    },
    SourceEntry {
        source: "home-assistant/v1",
        shape: Shape::State,
        key_recipe: "the entity id, authored in the webhook body",
        expires_at: Expiry::Never,
        retired_as: None,
    },
    SourceEntry {
        source: "github/v1",
        shape: Shape::Event,
        key_recipe: "the source's own event id",
        expires_at: Expiry::IfProvided("an expiry carried in the event payload, if any"),
        retired_as: None,
    },
    SourceEntry {
        source: "photo-site/v1",
        shape: Shape::Event,
        key_recipe: "the source's own event id",
        expires_at: Expiry::IfProvided("an expiry carried in the event payload, if any"),
        retired_as: None,
    },
    SourceEntry {
        source: "gmail-alert/v1",
        shape: Shape::Event,
        key_recipe: "the source's own message id",
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

    /// Frozen table-pin: every entry's `(source, shape, expires_at,
    /// retired_as)` tuple, verbatim, in registration order. `source` and
    /// `shape` are half of the `sha256(source, source_key)` alert id and
    /// drive the occurrence-lifecycle mechanism
    /// (`server/authority/src/handlers/alerts.rs`) — this is what actually
    /// catches `"gmail/v1"` silently drifting to `"gmail/v2"`, or
    /// `item-threshold/v1` silently flipping to `Shape::Event`, neither of
    /// which the version-suffix check or a single spot-checked entry would
    /// notice.
    #[test]
    fn registry_matches_the_frozen_adr_0014_table() {
        let expected: &[(&str, Shape, Expiry, Option<&str>)] = &[
            ("gmail/v1", Shape::Event, Expiry::Never, None),
            ("m365-mail/v1", Shape::Event, Expiry::Never, None),
            (
                "google-calendar/v1",
                Shape::Event,
                Expiry::Always("the instance's end time"),
                None,
            ),
            (
                "m365-calendar/v1",
                Shape::Event,
                Expiry::Always("the instance's end time"),
                None,
            ),
            (
                "city-waste/v1",
                Shape::Event,
                Expiry::Always("end of the affected collection date"),
                Some("city-waste/v2"),
            ),
            ("item-threshold/v1", Shape::State, Expiry::Never, None),
            ("healthchecks/v1", Shape::State, Expiry::Never, None),
            ("home-assistant/v1", Shape::State, Expiry::Never, None),
            (
                "github/v1",
                Shape::Event,
                Expiry::IfProvided("an expiry carried in the event payload, if any"),
                None,
            ),
            (
                "photo-site/v1",
                Shape::Event,
                Expiry::IfProvided("an expiry carried in the event payload, if any"),
                None,
            ),
            ("gmail-alert/v1", Shape::Event, Expiry::Never, None),
        ];

        assert_eq!(
            REGISTRY.len(),
            expected.len(),
            "registry gained or lost a source"
        );
        for (entry, (source, shape, expires_at, retired_as)) in REGISTRY.iter().zip(expected) {
            assert_eq!(entry.source, *source);
            assert_eq!(entry.shape, *shape, "{} shape drifted", entry.source);
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
            key_recipe: "<stream>:<scheduled-date>",
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
