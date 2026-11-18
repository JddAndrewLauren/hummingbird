//! The glance host (ADR-0041): a device with its own core and no screen,
//! which hands two computed views to Home Assistant over MQTT.
//!
//! This half is the host's decidable part, natively tested. It holds what
//! leaves Hummingbird ([`state_payload`]), how Home Assistant is told about
//! it ([`discovery`]), and the write transport that refuses every mutation
//! ([`RefusingWrite`]). `main.rs` only wires these to the core's sync
//! cycle, the Keychain, the clock and the broker.
//!
//! **Nothing here decides which item shows.** The Frontier is
//! `Core::frontier` in `frontier::by_priority_then_due` order, and Today is
//! `Core::today`. This crate maps their output to a payload (ADR-0025).

use async_trait::async_trait;
use hummingbird_core::decisions::frontier::{by_priority_then_due, FrontierItem};
use hummingbird_core::decisions::urgency::compute_urgency;
use hummingbird_core::sync::write::{MutationRequest, MutationTransport, RawResponse};
use hummingbird_core::sync::{CycleOutcome, TransportError};
use hummingbird_core::CoreCycleOutcome;
use hummingbird_domain::Item;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The longest list either view publishes. The panel fits fewer; the true
/// count travels beside the list so it can say "+N more".
pub const LIST_CAP: usize = 20;

/// The retained JSON state both views travel in.
pub const STATE_TOPIC: &str = "hummingbird/glance/state";

/// `online` while the host's MQTT session is up; the last will sets
/// `offline` when it drops.
pub const AVAILABILITY_TOPIC: &str = "hummingbird/glance/availability";

/// The host's cycle cadence. Its own loop owns it: the LaunchAgent sets no
/// interval (CLAUDE.md, "No competing clocks").
pub const CYCLE_INTERVAL_SECS: u64 = 5 * 60;

/// Three missed publishes. The host publishes after every cycle attempt,
/// completed or not, so this measures whether the host is alive. Whether
/// its data is fresh is `last_cycle`'s job.
pub const EXPIRE_AFTER_SECS: u64 = 3 * CYCLE_INTERVAL_SECS;

/// One item as it leaves Hummingbird: a title and its facts, never more
/// (ADR-0041 decision 4). No id, description, checklist or links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GlanceItem {
    pub title: String,
    pub context: Option<String>,
    pub size: Option<String>,
    pub energy: Option<String>,
    /// `compute_urgency`'s spelling: `calm`, `soon`, `now` or `overdue`.
    pub band: String,
    pub deadline: Option<String>,
    pub scheduled_date: Option<String>,
}

/// One view: its true count and at most [`LIST_CAP`] of its items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GlanceList {
    pub count: usize,
    pub items: Vec<GlanceItem>,
}

/// The whole retained state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GlanceState {
    pub frontier: GlanceList,
    pub today: GlanceList,
    /// When the last cycle completed, RFC 3339 with the host's offset.
    pub last_cycle: String,
}

fn glance_item(item: &Item, now: &str) -> GlanceItem {
    GlanceItem {
        title: item.title.clone(),
        context: item.context.clone(),
        size: item.size.map(|size| size.as_str().to_string()),
        energy: item.energy.map(|energy| energy.as_str().to_string()),
        band: compute_urgency(item.deadline.as_deref(), now).as_str().to_string(),
        deadline: item.deadline.clone(),
        scheduled_date: item.scheduled_date.clone(),
    }
}

fn glance_list<'a>(items: impl ExactSizeIterator<Item = &'a Item>, now: &str) -> GlanceList {
    let count = items.len();
    GlanceList {
        count,
        items: items.take(LIST_CAP).map(|item| glance_item(item, now)).collect(),
    }
}

/// `Core::frontier`'s items in the frontier's display order,
/// `by_priority_then_due`. The core returns the frontier unordered (keyed by
/// id), and the order is the shared decision every board reads.
pub fn frontier_in_display_order(frontier: Vec<Item>) -> Vec<Item> {
    let facts: Vec<FrontierItem> = frontier
        .iter()
        .map(|item| FrontierItem {
            id: item.id.clone(),
            priority: item.priority,
            deadline: item.deadline.clone(),
            context: item.context.clone(),
            size: item.size.map(|size| size.as_str().to_string()),
            energy: item.energy.map(|energy| energy.as_str().to_string()),
            project_id: item.project_id.clone(),
            created_at: item.created_at,
        })
        .collect();
    let mut by_id: std::collections::HashMap<String, Item> =
        frontier.into_iter().map(|item| (item.id.clone(), item)).collect();
    by_priority_then_due(&facts)
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect()
}

/// The state to publish: `frontier` already in display order (see
/// [`frontier_in_display_order`]), `today` as `Core::today` returned it.
/// `now` is the host's local wall clock in deadline shape, and `last_cycle`
/// is RFC 3339.
pub fn state_payload(frontier: &[Item], today: &[Item], now: &str, last_cycle: &str) -> GlanceState {
    GlanceState {
        frontier: glance_list(frontier.iter(), now),
        today: glance_list(today.iter(), now),
        last_cycle: last_cycle.to_string(),
    }
}

/// What the host does after one cycle attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterCycle {
    /// Publish the state, with the time of the last completed cycle.
    Publish,
    /// Publish nothing: no cycle has completed since the host started, so
    /// its mirror may be one it never pulled.
    Wait,
    /// The token was refused: publish `offline`, never an empty list.
    Offline,
}

/// Decides [`AfterCycle`] from one attempt's outcome, and records
/// `completed_at` in `last_completed` when the cycle completed. A failed pull
/// after a completed one still publishes: the last good lists, with the
/// older time, so the panel reads them as stale rather than gone.
pub fn after_cycle(
    outcome: &CoreCycleOutcome,
    last_completed: &mut Option<String>,
    completed_at: String,
) -> AfterCycle {
    match outcome {
        CoreCycleOutcome::Held | CoreCycleOutcome::Cycle(CycleOutcome::CredentialNeeded { .. }) => {
            return AfterCycle::Offline;
        }
        CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. }) => *last_completed = Some(completed_at),
        _ => {}
    }
    if last_completed.is_some() {
        AfterCycle::Publish
    } else {
        AfterCycle::Wait
    }
}

/// One retained discovery message: its topic and its JSON body.
#[derive(Debug, Clone, PartialEq)]
pub struct Discovery {
    pub topic: String,
    pub payload: Value,
}

/// Home Assistant MQTT discovery for the three entities, one device. The
/// lists ride as attributes, because a state holds at most 255 characters.
///
/// Each list is there twice: `items`, a list for Home Assistant itself, and
/// `items_json`, the same list as one JSON string. Home Assistant's ESPHome
/// integration hands a device an attribute as `str(value)`, so a list reaches
/// the panel as a Python repr; a string reaches it as the JSON it is.
pub fn discovery() -> Vec<Discovery> {
    let device = json!({
        "identifiers": ["hummingbird_glance"],
        "name": "Hummingbird Glance",
        "manufacturer": "Hummingbird",
        "model": "Glance host",
    });
    let entity = |key: &str, name: &str, value: &str, extra: Value| {
        let mut payload = json!({
            "name": name,
            "unique_id": format!("hummingbird_glance_{key}"),
            "default_entity_id": format!("sensor.hummingbird_glance_{key}"),
            "state_topic": STATE_TOPIC,
            "value_template": value,
            "availability_topic": AVAILABILITY_TOPIC,
            "expire_after": EXPIRE_AFTER_SECS,
            "device": device,
        });
        if let (Value::Object(payload), Value::Object(extra)) = (&mut payload, extra) {
            payload.extend(extra);
        }
        Discovery {
            topic: format!("homeassistant/sensor/hummingbird_glance/{key}/config"),
            payload,
        }
    };
    let list = |key: &str, name: &str| {
        entity(
            key,
            name,
            &format!("{{{{ value_json.{key}.count }}}}"),
            json!({
                "json_attributes_topic": STATE_TOPIC,
                "json_attributes_template": format!(
                    "{{{{ {{'items': value_json.{key}.items, 'items_json': value_json.{key}.items | tojson}} | tojson }}}}"
                ),
                "state_class": "measurement",
                "icon": if key == "today" { "mdi:calendar-today" } else { "mdi:format-list-checks" },
            }),
        )
    };
    vec![
        list("frontier", "Frontier"),
        list("today", "Today"),
        entity(
            "last_cycle",
            "Last cycle",
            "{{ value_json.last_cycle }}",
            json!({ "device_class": "timestamp" }),
        ),
    ]
}

/// The host's write transport: every mutation is answered 403 without
/// touching the network (ADR-0041 decision 2).
///
/// A 403 is a permanent write error to the core, so anything that somehow
/// reached the queue is dead-lettered and the pull still runs. A transport
/// error instead would block the queue, and with it every later pull.
#[derive(Debug, Default)]
pub struct RefusingWrite {
    refused: AtomicUsize,
}

impl RefusingWrite {
    /// How many mutations this transport has refused. Anything above zero
    /// is a bug in the host, and `main.rs` logs it.
    pub fn refused(&self) -> usize {
        self.refused.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl MutationTransport for RefusingWrite {
    async fn send(&self, _access_token: &str, _request: MutationRequest) -> Result<RawResponse, TransportError> {
        self.refused.fetch_add(1, Ordering::Relaxed);
        Ok(RawResponse {
            status: 403,
            body: r#"{"error":"the glance host never writes"}"#.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_core::sync::{ChangesTransport, DrainOutcome, Trigger};
    use hummingbird_core::{CaptureOptions, Core};
    use hummingbird_domain::{Energy, Size, Stage};

    const NOW: &str = "2026-11-16T09:00";

    fn item(id: &str, title: &str) -> Item {
        Item {
            id: id.into(),
            seq: Some(1),
            title: title.into(),
            description: Some("private notes".into()),
            stage: Stage::Ready,
            size: Some(Size::Quick),
            energy: Some(Energy::Low),
            context: Some("@computer".into()),
            priority: 0,
            project_id: Some("p-1".into()),
            project_pos: None,
            deadline: None,
            scheduled_date: None,
            source: Some("web/v1".into()),
            source_key: None,
            source_url: Some("https://example.com/private".into()),
            vault_path: Some("Notes/private.md".into()),
            link_url: Some("https://example.com/link".into()),
            link_label: None,
            archived_at: None,
            agent: false,
            created_at: 1,
            updated_at: 2,
            version: 3,
        }
    }

    #[test]
    fn each_item_carries_only_the_seven_allowed_fields() {
        let mut due = item("a-1", "Call the plumber");
        due.deadline = Some("2026-11-16T17:00".into());
        let state = state_payload(&[], &[due], NOW, "2026-11-16T09:00:00-08:00");
        let json = serde_json::to_value(&state.today.items[0]).unwrap();
        let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(
            keys,
            ["band", "context", "deadline", "energy", "scheduled_date", "size", "title"]
        );
        assert_eq!(json["title"], "Call the plumber");
        assert_eq!(json["band"], "now");
        assert_eq!(json["size"], "quick");
        assert_eq!(json["energy"], "low");
        let whole = serde_json::to_string(&state).unwrap();
        for private in ["private notes", "example.com", "Notes/private.md", "a-1", "p-1"] {
            assert!(!whole.contains(private), "{private} must never leave Hummingbird");
        }
    }

    #[test]
    fn a_long_list_is_capped_and_keeps_its_true_count() {
        let many: Vec<Item> = (0..LIST_CAP + 5).map(|n| item(&format!("a-{n}"), &format!("t{n}"))).collect();
        let state = state_payload(&many, &[], NOW, "x");
        assert_eq!(state.frontier.count, LIST_CAP + 5);
        assert_eq!(state.frontier.items.len(), LIST_CAP);
        assert_eq!(state.frontier.items[0].title, "t0", "the cap keeps the head of the order");
        assert_eq!(state.today.count, 0);
        assert!(state.today.items.is_empty());
    }

    #[test]
    fn the_frontier_publishes_in_the_shared_display_order() {
        let mut urgent = item("z", "urgent");
        urgent.priority = 1;
        let mut dated = item("b", "dated");
        dated.deadline = Some("2026-11-20".into());
        let plain = item("a", "plain");
        let ordered = frontier_in_display_order(vec![plain, dated, urgent]);
        let titles: Vec<&str> = ordered.iter().map(|item| item.title.as_str()).collect();
        assert_eq!(titles, ["urgent", "dated", "plain"]);
    }

    #[test]
    fn discovery_names_three_sensors_on_one_device_with_availability_and_expiry() {
        let messages = discovery();
        let topics: Vec<&str> = messages.iter().map(|m| m.topic.as_str()).collect();
        assert_eq!(
            topics,
            [
                "homeassistant/sensor/hummingbird_glance/frontier/config",
                "homeassistant/sensor/hummingbird_glance/today/config",
                "homeassistant/sensor/hummingbird_glance/last_cycle/config",
            ]
        );
        for message in &messages {
            let p = &message.payload;
            assert_eq!(p["device"]["identifiers"][0], "hummingbird_glance");
            assert_eq!(p["state_topic"], STATE_TOPIC);
            assert_eq!(p["availability_topic"], AVAILABILITY_TOPIC);
            assert_eq!(p["expire_after"], EXPIRE_AFTER_SECS);
        }
        assert_eq!(messages[0].payload["value_template"], "{{ value_json.frontier.count }}");
        assert_eq!(
            messages[1].payload["json_attributes_template"],
            "{{ {'items': value_json.today.items, 'items_json': value_json.today.items | tojson} | tojson }}"
        );
        assert_eq!(messages[2].payload["device_class"], "timestamp");
    }

    fn pull_failed() -> CoreCycleOutcome {
        CoreCycleOutcome::Cycle(CycleOutcome::PullFailed { drain: DrainOutcome::Completed { dead_lettered: 0 }, retry_after_ms: 1_000 })
    }

    fn completed() -> CoreCycleOutcome {
        CoreCycleOutcome::Cycle(CycleOutcome::Completed {
            drain: DrainOutcome::Completed { dead_lettered: 0 },
            active_item_count: 0,
            was_full_sweep: false,
        })
    }

    #[test]
    fn nothing_is_published_until_a_cycle_completes() {
        let mut last = None;
        assert_eq!(after_cycle(&pull_failed(), &mut last, "t1".into()), AfterCycle::Wait);
        assert_eq!(last, None);
        assert_eq!(after_cycle(&completed(), &mut last, "t2".into()), AfterCycle::Publish);
        assert_eq!(last.as_deref(), Some("t2"));
    }

    #[test]
    fn a_failed_pull_after_a_completed_one_publishes_with_the_older_time() {
        let mut last = Some("t1".to_string());
        assert_eq!(after_cycle(&pull_failed(), &mut last, "t2".into()), AfterCycle::Publish);
        assert_eq!(last.as_deref(), Some("t1"), "the time is the last completed cycle's");
    }

    #[test]
    fn a_refused_token_goes_offline_and_never_publishes_a_list() {
        let mut last = Some("t1".to_string());
        assert_eq!(after_cycle(&CoreCycleOutcome::Held, &mut last, "t2".into()), AfterCycle::Offline);
        let refused = CoreCycleOutcome::Cycle(CycleOutcome::CredentialNeeded { drain: DrainOutcome::Completed { dead_lettered: 0 } });
        assert_eq!(after_cycle(&refused, &mut last, "t3".into()), AfterCycle::Offline);
    }

    /// A pull that answers an empty delta, so a cycle can complete.
    struct EmptyRead;

    #[async_trait]
    impl ChangesTransport for EmptyRead {
        async fn fetch_changes(&self, _: &str, _: i64) -> Result<String, TransportError> {
            Ok(serde_json::to_string(&hummingbird_domain::ChangesResponse::empty(1)).unwrap())
        }
        async fn fetch_sweep(&self, _: &str) -> Result<String, TransportError> {
            Ok(serde_json::to_string(&hummingbird_domain::ChangesResponse::empty(1)).unwrap())
        }
    }

    #[tokio::test]
    async fn a_queued_mutation_is_refused_dead_lettered_and_the_pull_still_runs() {
        let mut core = Core::new();
        core.push_api_key("hb_test");
        core.capture("seed-1", "should never leave", Stage::Triage, 1_000, CaptureOptions::default(), None)
            .await
            .unwrap();
        assert_eq!(core.queue_depth(), 1);

        let write = RefusingWrite::default();
        let outcome = core.run(&EmptyRead, &write, 2_000, Trigger::User, true, 0.0).await;

        assert!(
            matches!(outcome, CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })),
            "the refusal must not block the pull: {outcome:?}"
        );
        assert_eq!(write.refused(), 1);
        assert_eq!(core.queue_depth(), 0);
        assert_eq!(core.dead_letters().len(), 1);
    }
}
