//! The frozen alert-source registry (ADR-0014). One entry per `source`
//! string ever written to `alerts.source`, `items.source` or
//! `context_snapshots.source` — the same frozen namespace, ADR-0009 rule 4.
//! `alerts.rs` mints an alert id as `sha256(source, source_key)`, so the
//! `source_key` recipe *is* occurrence identity: changing it silently
//! orphans every row already minted, or silently absorbs new occurrences
//! into stale ones. Neither symptom rings — this registry is `sweep.py`'s
//! frozen-namespace lesson re-learned for alerts.
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

/// One registered source: its frozen, versioned `source` string, its
/// declared [`Shape`], its `source_key` recipe (documentation — nothing
/// machine-checks it beyond the recipe function's own frozen test vector),
/// whether it declares an `expires_at` and what that derives from, and its
/// retirement state.
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
    /// What the source's `expires_at` derives from, if it declares one at
    /// all. `None` means the row never expires — it sits until acked (event
    /// sources) or resolved (state sources).
    pub expires_at: Option<&'static str>,
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

/// The frozen registry. Every entry's `source` carries a version suffix
/// (enforced by [`tests::every_registered_source_is_versioned`]); every
/// source below has at least one frozen key-vector test in this module.
pub const REGISTRY: &[SourceEntry] = &[
    SourceEntry {
        source: "gmail/v1",
        shape: Shape::Event,
        key_recipe: "the Gmail message id",
        expires_at: None,
        retired_as: None,
    },
    SourceEntry {
        source: "m365-mail/v1",
        shape: Shape::Event,
        key_recipe: "the mail's internetMessageId — never the Graph `id`, \
                      which changes on a folder move",
        expires_at: None,
        retired_as: None,
    },
    SourceEntry {
        source: "google-calendar/v1",
        shape: Shape::Event,
        key_recipe: "<eventId or recurringEventId>:<originalStartTime>",
        expires_at: Some("the instance's end time"),
        retired_as: None,
    },
    SourceEntry {
        source: "m365-calendar/v1",
        shape: Shape::Event,
        key_recipe: "<seriesMasterId or id>:<originalStart>",
        expires_at: Some("the instance's end time"),
        retired_as: None,
    },
    SourceEntry {
        source: "city-waste/v1",
        shape: Shape::Event,
        key_recipe: "<stream>:<scheduled-date> — the originally scheduled \
                      collection date, never whatever date a later \
                      correction slides to",
        expires_at: Some("end of the affected collection date"),
        retired_as: None,
    },
    SourceEntry {
        source: "item-threshold/v1",
        shape: Shape::State,
        key_recipe: "item:<id>",
        expires_at: None,
        retired_as: None,
    },
    SourceEntry {
        source: "healthchecks/v1",
        shape: Shape::State,
        key_recipe: "the check id, authored in the webhook body",
        expires_at: None,
        retired_as: None,
    },
    SourceEntry {
        source: "home-assistant/v1",
        shape: Shape::State,
        key_recipe: "the entity id, authored in the webhook body",
        expires_at: None,
        retired_as: None,
    },
    SourceEntry {
        source: "github/v1",
        shape: Shape::Event,
        key_recipe: "the source's own event id",
        expires_at: Some("none unless the event carries one"),
        retired_as: None,
    },
    SourceEntry {
        source: "photo-site/v1",
        shape: Shape::Event,
        key_recipe: "the source's own event id",
        expires_at: Some("none unless the event carries one"),
        retired_as: None,
    },
    SourceEntry {
        source: "gmail-alert/v1",
        shape: Shape::Event,
        key_recipe: "the source's own message id",
        expires_at: None,
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
    format!("{}:{}", recurring_event_id.unwrap_or(event_id), original_start_time)
}

/// `m365-calendar/v1`: `<seriesMasterId or id>:<originalStart>`.
pub fn m365_calendar_v1_key(id: &str, series_master_id: Option<&str>, original_start: &str) -> String {
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
            let has_version_suffix = entry
                .source
                .rsplit_once("/v")
                .is_some_and(|(_, suffix)| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()));
            assert!(has_version_suffix, "{} has no /vN suffix", entry.source);
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

    /// Acceptance: a retired source is representable, and distinguishable
    /// from an unknown one — `Some(entry)` with `is_retired() == true`
    /// versus `find`'s `None` for a string never registered at all. No
    /// shipped source is retired yet, so this exercises the mechanism
    /// directly on a locally built entry rather than on `REGISTRY`.
    #[test]
    fn a_retired_source_is_representable_and_distinct_from_unknown() {
        let retired = SourceEntry {
            source: "city-waste/v1",
            shape: Shape::Event,
            key_recipe: "<stream>:<scheduled-date>",
            expires_at: Some("end of the affected collection date"),
            retired_as: Some("city-waste/v2"),
        };
        assert!(retired.is_retired());
        assert_eq!(retired.retired_as, Some("city-waste/v2"));

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
    /// move — the Graph `id` isn't even a parameter this function can see.
    #[test]
    fn m365_mail_v1_key_is_unchanged_by_a_folder_move() {
        let internet_message_id = "<abc123@contoso.com>";
        let key_before_move = m365_mail_v1_key(internet_message_id);
        // The move changes the Graph `id` (not modeled here — the function
        // has no parameter for it), but `internetMessageId` survives.
        let key_after_move = m365_mail_v1_key(internet_message_id);
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
        assert_ne!(first, second, "two instances of one series must key differently");

        // The same occurrence, re-fetched (e.g. after a lost cursor):
        let first_again = google_calendar_v1_key("inst-1", Some("series-abc"), "2026-08-10T09:00:00");
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
    /// not remint the key, because the key is built from the originally
    /// scheduled date, a parameter the correction never touches.
    #[test]
    fn city_waste_v1_keys_on_the_scheduled_date_not_a_later_correction() {
        let originally_scheduled = "2026-08-17";
        let key_when_first_seen = city_waste_v1_key("trash", originally_scheduled);
        // The city corrects the pickup to slide from Tuesday to Wednesday;
        // the *scheduled* coordinate this key is built from never changes,
        // so the same call with the same fixed date reproduces the key.
        let key_after_correction = city_waste_v1_key("trash", originally_scheduled);
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
        assert_eq!(home_assistant_v1_key("binary_sensor.front_door"), "binary_sensor.front_door");
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
