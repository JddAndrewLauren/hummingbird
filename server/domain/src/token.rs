//! Per-writer bearer auth (ADR-0008): token scopes and the two shapes the
//! admin endpoints return. The `token_hash` column never leaves the server —
//! neither type carries it.

use serde::{Deserialize, Serialize};

/// What a bearer token may do. The serde string of each variant is
/// byte-for-byte the DDL `CHECK` literal — `hummingbird-authority` has a
/// test holding the two lists together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// A full task client: every task-lane write, alert dismiss, and reads.
    Device,
    /// The capture sweeper: `POST /api/items` only.
    Sweeper,
    /// A webhook source: `POST /api/alerts` only.
    Ingest,
}

impl Scope {
    pub const ALL: [Scope; 3] = [Scope::Device, Scope::Sweeper, Scope::Ingest];

    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Device => "device",
            Scope::Sweeper => "sweeper",
            Scope::Ingest => "ingest",
        }
    }

    pub fn parse(s: &str) -> Option<Scope> {
        Scope::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// Token metadata as the admin list/replay endpoints return it — everything
/// in the `tokens` row except the hash.
///
/// `source` (#145) binds an `ingest` token to exactly one webhook source —
/// `Some` for every `ingest` token, always `None` for `device`/`sweeper`.
///
/// `allowed_origin` (#845) is the one `https` origin an `ingest` token's
/// alert links must sit on, in the canonical form the mint stored — `None`
/// for a token that may link anywhere, and always for `device`/`sweeper`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenInfo {
    pub id: String,
    /// 'pixel-9', 'sweeper', 'home-assistant', …
    pub name: String,
    pub scope: Scope,
    pub source: Option<String>,
    #[serde(default)]
    pub allowed_origin: Option<String>,
    pub created_at: i64,
    pub last_seen: Option<i64>,
    pub revoked_at: Option<i64>,
}

/// The 201 mint response: [`TokenInfo`]'s identity fields plus the
/// plaintext token, shown exactly once — only its sha256 is stored, so a
/// replayed mint returns [`TokenInfo`] without it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MintedToken {
    pub id: String,
    pub name: String,
    pub scope: Scope,
    pub source: Option<String>,
    #[serde(default)]
    pub allowed_origin: Option<String>,
    pub created_at: i64,
    pub token: String,
}

/// The closed set of platforms a device signs in from (ADR-0040 decision 1)
/// — the middle segment of the token id `POST /api/auth/google` derives,
/// `device-<platform>-<slug of the name>`. Not [`crate::Platform`], which
/// is a *push* target's platform and a different set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevicePlatform {
    Web,
    Android,
    /// The browser capture extension (ADR-0037).
    Chrome,
}

impl DevicePlatform {
    pub const ALL: [DevicePlatform; 3] =
        [DevicePlatform::Web, DevicePlatform::Android, DevicePlatform::Chrome];

    pub fn as_str(self) -> &'static str {
        match self {
            DevicePlatform::Web => "web",
            DevicePlatform::Android => "android",
            DevicePlatform::Chrome => "chrome",
        }
    }

    pub fn parse(s: &str) -> Option<DevicePlatform> {
        DevicePlatform::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// The 201 of `POST /api/auth/google` (ADR-0040): the id the authority
/// chose and the plaintext token, shown exactly once — only its sha256 is
/// stored. Deliberately just the pair: the device already knows the name
/// and platform it sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedInDevice {
    pub id: String,
    pub token: String,
}

/// What `GET /api/devices` says a device is (ADR-0040 decision 6) — derived
/// from the token id, never stored, so a row minted before sign-in existed
/// (`device-mac`, `runner`) reads as [`DeviceListPlatform::Other`] rather
/// than failing to read. A wider set than [`DevicePlatform`], which is what a
/// device may *sign in* as: the watch gets its token by paste (ADR-0039) and
/// is listed, but can never be named in a sign-in body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceListPlatform {
    Web,
    Android,
    Chrome,
    Watch,
    Other,
}

impl DeviceListPlatform {
    /// `device-<platform>` exactly, or `device-<platform>-…` — the two
    /// shapes `POST /api/auth/google` derives (a name with no sluggable
    /// letters is the bare form), plus the watch's admin-minted
    /// `device-watch`. A bare string prefix would read `device-watchtower`
    /// as a watch; the dash keeps the platform a whole segment.
    pub fn of_token_id(id: &str) -> DeviceListPlatform {
        use DeviceListPlatform::*;
        [(Web, "web"), (Android, "android"), (Chrome, "chrome"), (Watch, "watch")]
            .into_iter()
            .find(|(_, segment)| {
                id.strip_prefix("device-")
                    .and_then(|rest| rest.strip_prefix(segment))
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with('-'))
            })
            .map_or(Other, |(platform, _)| platform)
    }
}

/// One row of `GET /api/devices` (ADR-0040 decision 6): a `device`-scope
/// token as another device sees it — [`TokenInfo`] without its scope and
/// source (always `device` and `None` here), plus the three facts the
/// listing derives. `is_current` is true only on the caller's own row;
/// `revocable` is false only for the holders `DELETE /api/devices/:id`
/// refuses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub platform: DeviceListPlatform,
    pub created_at: i64,
    pub last_seen: Option<i64>,
    pub revoked_at: Option<i64>,
    pub is_current: bool,
    pub revocable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_platform_derives_from_each_prefix_and_reads_legacy_ids() {
        use DeviceListPlatform::*;
        for (id, want) in [
            ("device-web-laptop", Web),
            ("device-web", Web),
            ("device-web-laptop-2", Web),
            ("device-android-pixel-fold", Android),
            ("device-android", Android),
            ("device-chrome-mac", Chrome),
            ("device-chrome", Chrome),
            ("device-watch", Watch),
            ("device-watch-2", Watch),
            // Legacy and non-sign-in holders: they still read.
            ("device-mac", Other),
            ("device-pixel", Other),
            ("runner", Other),
            ("openclaw-agent", Other),
            ("rig-device", Other),
            // The platform is a whole segment, not a string prefix.
            ("device-watchtower", Other),
            ("device-webby", Other),
            ("web-device", Other),
            ("", Other),
        ] {
            assert_eq!(DeviceListPlatform::of_token_id(id), want, "{id}");
        }
    }

    #[test]
    fn listed_platform_spellings_are_snake_case() {
        for (platform, wire) in [
            (DeviceListPlatform::Web, "\"web\""),
            (DeviceListPlatform::Android, "\"android\""),
            (DeviceListPlatform::Chrome, "\"chrome\""),
            (DeviceListPlatform::Watch, "\"watch\""),
            (DeviceListPlatform::Other, "\"other\""),
        ] {
            assert_eq!(serde_json::to_string(&platform).unwrap(), wire);
        }
    }

    #[test]
    fn device_platform_spellings_are_the_closed_set() {
        for platform in DevicePlatform::ALL {
            let wire = serde_json::to_string(&platform).unwrap();
            assert_eq!(wire, format!("\"{}\"", platform.as_str()));
            assert_eq!(DevicePlatform::parse(platform.as_str()), Some(platform));
        }
        assert_eq!(DevicePlatform::parse("ios"), None);
        assert!(serde_json::from_str::<DevicePlatform>("\"watch\"").is_err());
    }
}
