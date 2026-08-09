//! Alerts: the pushed-context lane. Row mapping first (the delta pull
//! reads it); the ingest upsert and device dismiss handlers join in the
//! alerts slice of #114.

use hummingbird_domain::Alert;

use crate::codec::RowReader;
use crate::sql::{Row, SqlError};

pub(super) fn alert_from_row(row: &Row) -> Result<Alert, SqlError> {
    let r = RowReader(row);
    Ok(Alert {
        id: r.text("id")?,
        source: r.text("source")?,
        source_key: r.text("source_key")?,
        title: r.text("title")?,
        body: r.opt_text("body"),
        url: r.opt_text("url"),
        severity: r.opt_text("severity"),
        raised_at: r.int("raised_at")?,
        resolved_at: r.opt_int("resolved_at"),
        dismissed_at: r.opt_int("dismissed_at"),
        expires_at: r.opt_int("expires_at"),
        version: r.int("version")?,
    })
}
