//! The words for a run that never reached the seam (#273, sunk here from
//! `client/web/src/skills/decline.ts` at #538).
//!
//! **Only for when the transport speaks.** A 200 whose terminal line says
//! `ok:false` does not come here: that is the seam declining, and its prose
//! is rendered verbatim ([`super::run::reduce_run`] takes the line's
//! `error` as-is). #307 made the seam's decline prose-only, with no
//! machine-readable reason code, precisely so nothing string-matches it —
//! prefixing or rewording it here would be the same mistake in a different
//! place.
//!
//! So this module covers exactly three cases the seam never sees: no
//! credential to send, a request that never resolved, and a response whose
//! body the client could not read as a terminal line. Every client's
//! transport reports which of the three happened to *its* socket; none of
//! them holds a copy of these sentences.

/// No device token on this device. The one decline that names an action,
/// because the user has one to take and the app can point at it.
///
/// The wording says "Settings" because that is what the web calls the
/// surface; Android's own token entry is on its Status screen, and #538
/// deliberately did **not** fork the sentence per platform — one decline,
/// one wording, and a shell that puts its token entry where the sentence
/// says it is.
pub const NO_TOKEN: &str = "No device token on this device. Enter one in Settings, then try again.";

/// The stream ended with no terminal line — a connection dropped mid-run,
/// or a body that was not what it claimed. The work may or may not have
/// landed: the runner writes to the authority, so a checklist can still
/// appear at the next sync. Said plainly rather than guessed at either way.
pub const NO_TERMINAL_LINE: &str = "The run ended without an answer.";

/// A request that never resolved — offline, DNS, a connection reset. The
/// caller's own transport-layer detail rides along; nothing here parses it.
pub fn decline_for_transport(detail: &str) -> String {
    let trimmed = detail.trim();
    if trimmed.is_empty() {
        "Could not reach the server.".to_string()
    } else {
        format!("Could not reach the server: {trimmed}")
    }
}

/// A non-2xx whose body carried no readable terminal line. The proxy
/// answers its own failures *with* one (ADR-0018's status table), so in
/// practice this is the 401/403 — empty bodies by design — and anything the
/// platform generated in front of the worker.
///
/// A 401 is the only status worth naming as itself: it means this device's
/// token, which the user can fix. Everything else is reported as the number
/// it was, because inventing a cause for a 500 would be a guess.
pub fn decline_for_response(status: u16) -> String {
    match status {
        401 => "Your device token was rejected. Re-enter it in Settings.".to_string(),
        403 => "This device's token is not allowed to run skills.".to_string(),
        other => format!("The server answered {other}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transport_detail_rides_along_verbatim() {
        assert_eq!(decline_for_transport("Failed to fetch"), "Could not reach the server: Failed to fetch");
    }

    #[test]
    fn an_empty_or_blank_detail_says_the_short_sentence() {
        assert_eq!(decline_for_transport(""), "Could not reach the server.");
        assert_eq!(decline_for_transport("   \n"), "Could not reach the server.");
    }

    #[test]
    fn the_two_credential_statuses_are_named_and_everything_else_is_its_number() {
        assert_eq!(decline_for_response(401), "Your device token was rejected. Re-enter it in Settings.");
        assert_eq!(decline_for_response(403), "This device's token is not allowed to run skills.");
        assert_eq!(decline_for_response(500), "The server answered 500.");
        assert_eq!(decline_for_response(404), "The server answered 404.");
    }

    /// These sentences exist so no client writes its own. Pinning them by
    /// value is what makes the web's literal TS copies (kept for
    /// module-evaluation order, see ADR-0025's #538 amendment) provably the
    /// same strings rather than merely similar ones.
    #[test]
    fn the_two_constants_are_the_sentences_every_client_pins() {
        assert_eq!(NO_TOKEN, "No device token on this device. Enter one in Settings, then try again.");
        assert_eq!(NO_TERMINAL_LINE, "The run ended without an answer.");
    }
}
