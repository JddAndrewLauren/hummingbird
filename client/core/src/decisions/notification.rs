//! Where a tapped notification lands (ADR-0027), decided from the push
//! payload alone.
//!
//! The alternative was to resolve this from the mirror — take the alert id
//! the tap hands over, look the row up, route on its `source_key`. ADR-0027
//! rejects that because the race is worst exactly where it matters most:
//! the window `AlertDetailViewModel` already documents ("the push arrives,
//! the user taps immediately, and the cycle the push enqueued has not
//! landed") is *most* likely to be open for an urgent, just-rung alert, so
//! a mirror-resolved destination would be least reliable for the highest
//! tier. The payload carrying its own subject removes the window rather
//! than narrowing it.
//!
//! That is also what keeps this module's contract intact: over two strings
//! the answer needs no [`crate::Core`], no mirror and no clock, so it stays
//! synchronously callable from a deep-link collector on the main thread.

use hummingbird_domain::{item_threshold_v1_item_id, ITEM_THRESHOLD_V1};

/// What a tapped notification should open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TapTarget {
    /// Item detail for `item_id` — the alert named a thing, so the tap
    /// opens the thing rather than the reading of it.
    Item { item_id: String },
    /// Alert detail. The answer for every source naming no item, and the
    /// honest fallback whenever the payload does not say.
    Alert,
}

/// Decide where a tap lands from a push payload's `source` and
/// `source_key`.
///
/// **Deliberately not named `destination`.** That word is taken twice
/// already — a Route's destination, and Triage's — and neither is this.
///
/// `Item` requires both that the source is the one state source naming an
/// item and that its key actually yields an id through the domain's
/// sanctioned inverse. Anything else — an unrecognised source, a
/// recognised one naming no item, a malformed key under the right source,
/// or the empty strings an old server's payload leaves behind — is
/// [`TapTarget::Alert`]. That fallback is the **permanent contract**, not a
/// version-skew allowance: seven of the eight registered sources name no
/// item, so "I cannot tell you what this is about, here is the alert" is
/// the honest steady-state answer.
///
/// **Why this is a free function and not a per-row decision.** The mobile
/// seam otherwise hands Kotlin decided records rather than predicates,
/// because each predicate call costs a JNI crossing per row. This one runs
/// **once per tap**, before navigation, and there is no record to carry a
/// decided answer on — the tap has two payload strings and no alert row
/// yet. It is the same per-gesture carve-out
/// [`super::capture::can_submit_capture`] took.
pub fn notification_tap_target(source: &str, source_key: &str) -> TapTarget {
    if source != ITEM_THRESHOLD_V1 {
        return TapTarget::Alert;
    }
    match item_threshold_v1_item_id(source_key) {
        Some(item_id) => TapTarget::Item { item_id: item_id.to_string() },
        None => TapTarget::Alert,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_domain::item_threshold_v1_key;

    #[test]
    fn an_item_threshold_alert_opens_its_item() {
        assert_eq!(
            notification_tap_target(ITEM_THRESHOLD_V1, &item_threshold_v1_key("item-42")),
            TapTarget::Item { item_id: "item-42".into() }
        );
    }

    /// The key is built through the domain recipe, never spelled here: a
    /// literal would be the second copy of the convention ADR-0027 part 2
    /// exists to prevent, and would keep this test green through exactly
    /// the recipe move that breaks the lane.
    #[test]
    fn another_source_opens_the_alert_even_with_an_item_shaped_key() {
        assert_eq!(
            notification_tap_target("city-waste/v2", &item_threshold_v1_key("item-42")),
            TapTarget::Alert
        );
    }

    #[test]
    fn a_malformed_key_under_the_right_source_opens_the_alert() {
        for key in ["item-42", "item", "item:", "", "ITEM:item-42"] {
            assert_eq!(
                notification_tap_target(ITEM_THRESHOLD_V1, key),
                TapTarget::Alert,
                "{key:?} names no item"
            );
        }
    }

    /// What an old server's payload looks like: the fields are absent, and
    /// the seam hands empty strings across rather than nulls.
    #[test]
    fn an_empty_payload_opens_the_alert() {
        assert_eq!(notification_tap_target("", ""), TapTarget::Alert);
    }
}
