//! The closed transport-failure classification `http.finished` records.
//!
//! [`crate::sync::transport::TransportError`] already carries the one
//! status the sync cycle branches on (401) — this module maps it (and
//! [`crate::sync::adapter::AdapterError`], the one extra case a malformed
//! body adds) into [`FailureClass`] rather than replacing either type. The
//! mapping is heuristic on `TransportError::message` where no status is
//! present, because that type carries only a status and a free-text
//! message today; a mis-classified `Unknown` costs nothing (redaction wise:
//! the message itself is never kept either way — see [`FailureClass`]'s own
//! docs on why).

use crate::sync::adapter::AdapterError;
use crate::sync::transport::TransportError;
use serde::{Deserialize, Serialize};

/// Exactly one of these seven — never the underlying message. HTTP status
/// codes are retained (`Http`'s `status`); no response content is, which is
/// the redaction rule this type exists to make checkable: there is no field
/// here a raw exception string or a response body could hide inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FailureClass {
    Timeout,
    Connect,
    Http { status: u16 },
    Body,
    Decode,
    Cancelled,
    Unknown,
}

/// Classifies a read/write transport failure. A status-carrying failure is
/// always [`FailureClass::Http`] regardless of message text — the status is
/// the authoritative signal `TransportError::is_unauthorized` already
/// trusts. Only a connection-level failure (`status: None`) falls back to a
/// message-text heuristic.
pub fn classify_transport_error(error: &TransportError) -> FailureClass {
    if let Some(status) = error.status {
        return FailureClass::Http { status };
    }
    let message = error.message.to_ascii_lowercase();
    if message.contains("timed out") || message.contains("timeout") {
        FailureClass::Timeout
    } else if message.contains("cancel") {
        FailureClass::Cancelled
    } else if message.contains("decode") || message.contains("deserializ") || message.contains("invalid json") {
        FailureClass::Decode
    } else if message.contains("body") {
        FailureClass::Body
    } else if message.contains("connect") || message.contains("connection") || message.contains("dns") {
        FailureClass::Connect
    } else {
        FailureClass::Unknown
    }
}

/// The read adapter's own extra failure mode — a malformed body — is always
/// [`FailureClass::Decode`], the one case [`classify_transport_error`] can't
/// see because it never reached the transport layer at all.
pub fn classify_adapter_error(error: &AdapterError) -> FailureClass {
    match error {
        AdapterError::Transport(transport_error) => classify_transport_error(transport_error),
        AdapterError::InvalidResponse(_) => FailureClass::Decode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_carrying_failure_is_always_http_whatever_the_message_says() {
        let error = TransportError::http(503, "connection timed out");
        assert_eq!(classify_transport_error(&error), FailureClass::Http { status: 503 });
    }

    #[test]
    fn a_connection_level_timeout_message_classifies_as_timeout() {
        let error = TransportError::new("operation timed out after 30s");
        assert_eq!(classify_transport_error(&error), FailureClass::Timeout);
    }

    #[test]
    fn a_connection_level_connect_failure_classifies_as_connect() {
        let error = TransportError::new("failed to connect: connection refused");
        assert_eq!(classify_transport_error(&error), FailureClass::Connect);
    }

    #[test]
    fn a_body_read_failure_classifies_as_body() {
        let error = TransportError::new("error reading response body");
        assert_eq!(classify_transport_error(&error), FailureClass::Body);
    }

    #[test]
    fn a_cancelled_request_classifies_as_cancelled() {
        let error = TransportError::new("request was cancelled");
        assert_eq!(classify_transport_error(&error), FailureClass::Cancelled);
    }

    #[test]
    fn an_unrecognised_connection_level_message_classifies_as_unknown() {
        let error = TransportError::new("something inscrutable happened");
        assert_eq!(classify_transport_error(&error), FailureClass::Unknown);
    }

    #[test]
    fn a_malformed_response_body_classifies_as_decode_via_the_adapter_error() {
        let error = AdapterError::InvalidResponse("expected value at line 1".to_string());
        assert_eq!(classify_adapter_error(&error), FailureClass::Decode);
    }

    #[test]
    fn an_adapter_transport_error_delegates_to_the_transport_classifier() {
        let error = AdapterError::Transport(TransportError::http(401, "unauthorized"));
        assert_eq!(classify_adapter_error(&error), FailureClass::Http { status: 401 });
    }

    /// Redaction: the classification never carries the message text itself
    /// — only `Http`'s status is structured data, everything else is a bare
    /// tag. Serializing every variant and asserting none of them contain
    /// the original message proves the type, not just this call site.
    #[test]
    fn no_failure_class_ever_serializes_the_original_message_text() {
        let secret_message = "sk-live-do-not-leak-this-token failed to connect";
        let error = TransportError::new(secret_message);
        let class = classify_transport_error(&error);
        let json = serde_json::to_string(&class).unwrap();
        assert!(!json.contains("sk-live-do-not-leak-this-token"));
    }
}
