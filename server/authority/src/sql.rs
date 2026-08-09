//! The storage seam: one synchronous SQL statement at a time. The Durable
//! Object is single-threaded and its SQLite API synchronous, and so is
//! rusqlite — a sync trait spares every handler the `async_trait` dance and
//! keeps meta-bump + row-write inside one event-loop turn, which is what
//! the DO's write coalescing makes atomic.

use std::collections::HashMap;

/// The SQLite value space, as both parameter and result cell.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

impl SqlValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            SqlValue::Integer(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            SqlValue::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn from_opt_i64(v: Option<i64>) -> SqlValue {
        v.map_or(SqlValue::Null, SqlValue::Integer)
    }

    pub fn from_opt_text(v: Option<&str>) -> SqlValue {
        v.map_or(SqlValue::Null, |s| SqlValue::Text(s.to_string()))
    }
}

/// One result row, keyed by column name — named over positional so the two
/// backends (rusqlite, the DO's SqlStorage) can't drift on column order.
pub type Row = HashMap<String, SqlValue>;

#[derive(Debug)]
pub struct SqlError {
    pub message: String,
}

/// Executes one parameterized statement and returns every result row.
pub trait Sql {
    fn exec(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, SqlError>;
}
