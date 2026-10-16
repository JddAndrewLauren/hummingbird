//! Signed-in devices' decision half (#839, ADR-0040 decision 7, ADR-0025):
//! which of `GET /api/devices`' rows the section shows, in what order, in
//! what words, what the sign-out confirmations say, and what a refused
//! Google sign-in says — so the web and the phone describe the operator's
//! devices identically (#887). The rows arrive from
//! [`crate::Core::devices`]; nothing here fetches.
//!
//! The capture extension cannot call the core, so it re-states
//! [`sign_in_refusal`]'s four sentences once, in
//! `tools/browser-capture/lib.js`, pinned by the same cases as this
//! module's tests (ADR-0037's re-statement rule). A wording change here is
//! a change there.
//!
//! Pure and clock-free like every module beside it: `now_ms` is an
//! argument, and ages read through [`super::settings::relative_age`], the
//! one relative-age rule the sync card already uses.

use hummingbird_domain::{DeviceInfo, DeviceListPlatform};
use serde::Serialize;

use super::settings::relative_age;
use crate::devices::SignInOutcome;

/// The words on this device's own row.
pub const THIS_DEVICE: &str = "This device";

/// One Signed-in devices row, decided. `name` is the operator's own label,
/// verbatim. `this_device` is [`THIS_DEVICE`] on the caller's row and
/// `None` on every other; `revocable` is the authority's own verdict
/// (false only for the holders `DELETE /api/devices/:id` refuses), so a
/// client draws no sign-out control the route would answer 403.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRow {
    pub id: String,
    pub name: String,
    pub is_current: bool,
    pub this_device: Option<String>,
    pub platform_label: String,
    pub last_seen_label: String,
    pub revocable: bool,
}

/// What a device is called by kind. `Other` is every id that predates
/// sign-in or never came through it (`device-mac`, `runner`).
pub fn platform_label(platform: DeviceListPlatform) -> &'static str {
    match platform {
        DeviceListPlatform::Web => "Web",
        DeviceListPlatform::Android => "Android",
        DeviceListPlatform::Chrome => "Chrome extension",
        DeviceListPlatform::Watch => "Watch",
        DeviceListPlatform::Other => "Other",
    }
}

/// `Last seen 3h ago`, or `Never seen` for a token that has made no
/// request yet.
pub fn last_seen_label(last_seen: Option<i64>, now_ms: i64) -> String {
    match last_seen {
        Some(at) => format!("Last seen {}", relative_age(now_ms - at)),
        None => "Never seen".to_string(),
    }
}

/// The section's rows: revoked tokens left out (a signed-out device is not
/// signed in), this device first, then most recently seen first — a
/// device never seen after every seen one — with the newest mint, then the
/// id, breaking ties so two clients never disagree on an order.
pub fn device_rows(devices: &[DeviceInfo], now_ms: i64) -> Vec<DeviceRow> {
    let mut live: Vec<&DeviceInfo> = devices.iter().filter(|d| d.revoked_at.is_none()).collect();
    live.sort_by(|a, b| {
        b.is_current
            .cmp(&a.is_current)
            .then_with(|| b.last_seen.cmp(&a.last_seen))
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    live.into_iter()
        .map(|device| DeviceRow {
            id: device.id.clone(),
            name: device.name.clone(),
            is_current: device.is_current,
            this_device: device.is_current.then(|| THIS_DEVICE.to_string()),
            platform_label: platform_label(device.platform).to_string(),
            last_seen_label: last_seen_label(device.last_seen, now_ms),
            revocable: device.revocable,
        })
        .collect()
}

/// The confirmation before this device signs itself out. Signing out
/// clears the credential and keeps the outbound queue (ADR-0004), so it
/// names how many unsent changes stay behind to send after the next
/// sign-in.
pub fn sign_out_confirmation(queue_depth: u32) -> String {
    match queue_depth {
        0 => "Sign out this device? Nothing is waiting to send.".to_string(),
        1 => "Sign out this device? 1 unsent change stays queued and sends once you sign in again."
            .to_string(),
        n => format!(
            "Sign out this device? {n} unsent changes stay queued and send once you sign in again."
        ),
    }
}

/// The confirmation before signing out another device. Its token is
/// revoked at once, so it says what that does to the device rather than
/// what stays queued — the queue is that device's own.
pub fn other_device_sign_out_confirmation(name: &str) -> String {
    format!("Sign out {name}? It stops syncing until it signs in again.")
}

/// What a refused Google sign-in says, one sentence per refusal the route
/// gives (403, 429, and the two 503s the lane already split), or `None`
/// for an outcome that is not a refusal: a sign-in, and the two arms that
/// carry their own detail (`Invalid`, `Failed`), which each client words
/// around that detail. No wildcard, so an arm added to [`SignInOutcome`]
/// is a compile error here rather than a silent `None`.
pub fn sign_in_refusal(outcome: &SignInOutcome) -> Option<&'static str> {
    match outcome {
        SignInOutcome::NotAllowed => {
            Some("That Google account isn't allowed to sign in here. Nothing on this device changed.")
        }
        SignInOutcome::RateLimited => {
            Some("Too many sign-ins in the last hour. Try again later, or paste a token instead.")
        }
        SignInOutcome::Unconfigured => {
            Some("Sign-in isn't set up on the authority yet. Paste a token instead.")
        }
        SignInOutcome::Unavailable => {
            Some("The authority couldn't check with Google just now. Try again in a minute.")
        }
        SignInOutcome::SignedIn(_) | SignInOutcome::Invalid { .. } | SignInOutcome::Failed { .. } => {
            None
        }
    }
}

/// [`sign_in_refusal`] keyed by the arm's wire name
/// ([`SignInOutcome::kind`]), for the web seam, whose sign-in answers
/// arrive as kinds. `None` for any kind that is not one of the four
/// refusals.
pub fn sign_in_refusal_for_kind(kind: &str) -> Option<&'static str> {
    [
        SignInOutcome::NotAllowed,
        SignInOutcome::RateLimited,
        SignInOutcome::Unconfigured,
        SignInOutcome::Unavailable,
    ]
    .iter()
    .find(|outcome| outcome.kind() == kind)
    .and_then(sign_in_refusal)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;
    const NOW: i64 = 100 * HOUR;

    fn device(id: &str, last_seen: Option<i64>, is_current: bool) -> DeviceInfo {
        DeviceInfo {
            id: id.to_string(),
            name: format!("{id} name"),
            platform: DeviceListPlatform::of_token_id(id),
            created_at: 0,
            last_seen,
            revoked_at: None,
            is_current,
            revocable: true,
        }
    }

    fn ids(rows: &[DeviceRow]) -> Vec<&str> {
        rows.iter().map(|row| row.id.as_str()).collect()
    }

    #[test]
    fn this_device_comes_first_then_the_most_recently_seen() {
        let rows = device_rows(
            &[
                device("device-android-pixel", Some(NOW - 5 * HOUR), false),
                device("device-chrome-mac", None, false),
                device("device-web-laptop", Some(NOW - 2 * HOUR), true),
                device("device-watch", Some(NOW - HOUR), false),
            ],
            NOW,
        );
        assert_eq!(
            ids(&rows),
            ["device-web-laptop", "device-watch", "device-android-pixel", "device-chrome-mac"],
            "this device first even when another was seen more recently; never seen last"
        );
    }

    #[test]
    fn ties_break_on_the_newest_mint_then_the_id() {
        let mut older = device("device-web-b", Some(NOW), false);
        older.created_at = 1;
        let mut newer = device("device-web-c", Some(NOW), false);
        newer.created_at = 2;
        let same_as_newer = DeviceInfo {
            id: "device-web-a".to_string(),
            ..newer.clone()
        };
        let rows = device_rows(&[older, same_as_newer, newer], NOW);
        assert_eq!(ids(&rows), ["device-web-a", "device-web-c", "device-web-b"]);
    }

    #[test]
    fn a_revoked_device_is_not_listed() {
        let mut revoked = device("device-android-old", Some(NOW), false);
        revoked.revoked_at = Some(NOW - HOUR);
        let rows = device_rows(&[revoked, device("device-web", Some(NOW), true)], NOW);
        assert_eq!(ids(&rows), ["device-web"]);
    }

    #[test]
    fn each_row_is_worded_in_the_core() {
        let mut runner = device("runner", Some(NOW - 3 * 24 * HOUR), false);
        runner.revocable = false;
        let rows = device_rows(
            &[
                device("device-web-laptop", Some(NOW - 30_000), true),
                device("device-chrome-mac", Some(NOW - 12 * 60_000), false),
                runner,
            ],
            NOW,
        );
        assert_eq!(
            rows[0],
            DeviceRow {
                id: "device-web-laptop".to_string(),
                name: "device-web-laptop name".to_string(),
                is_current: true,
                this_device: Some("This device".to_string()),
                platform_label: "Web".to_string(),
                last_seen_label: "Last seen just now".to_string(),
                revocable: true,
            }
        );
        assert_eq!(rows[1].this_device, None);
        assert_eq!(rows[1].platform_label, "Chrome extension");
        assert_eq!(rows[1].last_seen_label, "Last seen 12m ago");
        assert_eq!(rows[2].platform_label, "Other");
        assert_eq!(rows[2].last_seen_label, "Last seen 3d ago");
        assert!(!rows[2].revocable, "the authority's verdict, carried through");
    }

    #[test]
    fn every_platform_has_its_label() {
        for (platform, label) in [
            (DeviceListPlatform::Web, "Web"),
            (DeviceListPlatform::Android, "Android"),
            (DeviceListPlatform::Chrome, "Chrome extension"),
            (DeviceListPlatform::Watch, "Watch"),
            (DeviceListPlatform::Other, "Other"),
        ] {
            assert_eq!(platform_label(platform), label);
        }
    }

    #[test]
    fn a_device_never_seen_says_so() {
        assert_eq!(last_seen_label(None, NOW), "Never seen");
        assert_eq!(last_seen_label(Some(NOW - 2 * HOUR), NOW), "Last seen 2h ago");
    }

    #[test]
    fn the_sign_out_confirmation_names_what_stays_queued() {
        assert_eq!(
            sign_out_confirmation(0),
            "Sign out this device? Nothing is waiting to send."
        );
        assert_eq!(
            sign_out_confirmation(1),
            "Sign out this device? 1 unsent change stays queued and sends once you sign in again."
        );
        assert_eq!(
            sign_out_confirmation(4),
            "Sign out this device? 4 unsent changes stay queued and send once you sign in again."
        );
    }

    /// The extension re-states these four (`tools/browser-capture/
    /// signin.test.js` pins the same strings): change both together.
    const REFUSALS: [(&str, &str); 4] = [
        (
            "not_allowed",
            "That Google account isn't allowed to sign in here. Nothing on this device changed.",
        ),
        (
            "rate_limited",
            "Too many sign-ins in the last hour. Try again later, or paste a token instead.",
        ),
        ("unconfigured", "Sign-in isn't set up on the authority yet. Paste a token instead."),
        (
            "unavailable",
            "The authority couldn't check with Google just now. Try again in a minute.",
        ),
    ];

    #[test]
    fn each_refused_sign_in_has_its_one_sentence() {
        let outcomes = [
            SignInOutcome::NotAllowed,
            SignInOutcome::RateLimited,
            SignInOutcome::Unconfigured,
            SignInOutcome::Unavailable,
        ];
        for (outcome, (kind, sentence)) in outcomes.iter().zip(REFUSALS) {
            assert_eq!(outcome.kind(), kind);
            assert_eq!(sign_in_refusal(outcome), Some(sentence), "{kind}");
            assert_eq!(sign_in_refusal_for_kind(kind), Some(sentence), "{kind}");
        }
    }

    #[test]
    fn an_outcome_that_is_not_a_refusal_has_no_refusal_sentence() {
        let signed_in = SignInOutcome::SignedIn(hummingbird_domain::SignedInDevice {
            id: "device-web".to_string(),
            token: "hb_x".to_string(),
        });
        for outcome in [
            signed_in,
            SignInOutcome::Invalid { message: "too long".to_string() },
            SignInOutcome::Failed { message: "offline".to_string() },
        ] {
            assert_eq!(sign_in_refusal(&outcome), None, "{}", outcome.kind());
            assert_eq!(sign_in_refusal_for_kind(outcome.kind()), None, "{}", outcome.kind());
        }
        assert_eq!(sign_in_refusal_for_kind("busy"), None);
    }

    #[test]
    fn the_other_device_confirmation_names_it_and_what_signing_it_out_does() {
        assert_eq!(
            other_device_sign_out_confirmation("Pixel 10 Pro Fold"),
            "Sign out Pixel 10 Pro Fold? It stops syncing until it signs in again."
        );
    }

    #[test]
    fn a_row_serializes_camel_cased_for_the_web_seam() {
        let rows = device_rows(&[device("device-web", None, true)], NOW);
        let json = serde_json::to_value(&rows[0]).unwrap();
        for key in ["id", "name", "isCurrent", "thisDevice", "platformLabel", "lastSeenLabel", "revocable"] {
            assert!(json.get(key).is_some(), "{key}");
        }
    }
}
