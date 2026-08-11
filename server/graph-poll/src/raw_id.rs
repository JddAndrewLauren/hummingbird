//! A best-effort id for a permanently-skipped item —
//! `calendar_poll::stream::raw_event_id`'s discipline, shared here because
//! both of this crate's streams (`mail_stream.rs`, `calendar_stream.rs`)
//! skip past items their typed parse rejects. The delta cursor advances
//! past a skipped item permanently, so the skip log line is the only trace
//! that item ever existed — and the parse errors that reach it
//! (`MissingField`/`BadTimestamp`) mostly arrive from bodies that DO carry
//! an `"id"` (only some other field is missing or malformed). Reading it
//! off the raw JSON directly — bypassing the very parse that just failed —
//! recovers the real id in exactly those cases; `"?"` survives only when
//! even that read comes up empty (a body too malformed to hold an id at
//! all).

/// Reads `"id"` straight off a raw item body, falling back to `"?"` only
/// when the body is not JSON or holds no string `id`.
pub(crate) fn raw_item_id(raw: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v.get("id").and_then(|id| id.as_str()).map(str::to_string))
        .unwrap_or_else(|| "?".to_string())
}
