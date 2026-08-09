//! The row codec: small helpers each entity module builds its hand-written
//! `x_from_row` / patch-UPDATE on. Deliberately not a generic table codec —
//! every table's column list stays flat and greppable against its DDL.

use crate::sql::{Row, SqlError, SqlValue};

pub fn bad_cell(col: &str) -> SqlError {
    SqlError {
        message: format!("column `{col}` missing or mistyped"),
    }
}

/// Typed accessors over one row, erroring with the offending column name.
pub struct RowReader<'a>(pub &'a Row);

impl RowReader<'_> {
    pub fn text(&self, col: &str) -> Result<String, SqlError> {
        self.opt_text(col).ok_or_else(|| bad_cell(col))
    }

    pub fn opt_text(&self, col: &str) -> Option<String> {
        self.0.get(col).and_then(|v| v.as_text().map(str::to_string))
    }

    pub fn int(&self, col: &str) -> Result<i64, SqlError> {
        self.opt_int(col).ok_or_else(|| bad_cell(col))
    }

    pub fn opt_int(&self, col: &str) -> Option<i64> {
        self.0.get(col).and_then(SqlValue::as_i64)
    }

    /// An INTEGER 0/1 column (`steps.done`); anything else is a bad cell.
    pub fn bool_int(&self, col: &str) -> Result<bool, SqlError> {
        match self.int(col)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(bad_cell(col)),
        }
    }
}

/// Accumulates a PATCH's absolute-value assignments and renders its UPDATE.
/// Columns and params stay in lockstep by construction.
pub struct Sets {
    cols: Vec<&'static str>,
    params: Vec<SqlValue>,
}

impl Sets {
    pub fn new() -> Sets {
        Sets {
            cols: Vec::new(),
            params: Vec::new(),
        }
    }

    pub fn set(&mut self, col: &'static str, value: SqlValue) {
        self.cols.push(col);
        self.params.push(value);
    }

    pub fn is_empty(&self) -> bool {
        self.cols.is_empty()
    }

    /// `UPDATE {table} SET c1 = ?, … WHERE {pk_clause}` — the caller appends
    /// the pk params after [`Sets::into_params`].
    pub fn update_sql(&self, table: &str, pk_clause: &str) -> String {
        let assignments: Vec<String> = self.cols.iter().map(|c| format!("{c} = ?")).collect();
        format!(
            "UPDATE {table} SET {} WHERE {pk_clause}",
            assignments.join(", ")
        )
    }

    pub fn into_params(self) -> Vec<SqlValue> {
        self.params
    }
}
