//! The SQLite database of the lapp, in its data directory.
//!
//! ```ignore
//! database::execute("INSERT INTO tasks(description) VALUES (?1)", &["Buy milk".into()])?;
//! let rows = database::query("SELECT rowid, description FROM tasks", &[])?;
//! ```

use std::fmt;

use crate::bindings::laplace::lapp::database as wit;

/// A value of a column or of a parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Value {
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_real(&self) -> Option<f64> {
        match self {
            Self::Real(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            Self::Blob(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Self::Integer(value.into())
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Integer(value.into())
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::Real(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}

impl From<Vec<u8>> for Value {
    fn from(value: Vec<u8>) -> Self {
        Self::Blob(value)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Null, Into::into)
    }
}

impl From<wit::Value> for Value {
    fn from(value: wit::Value) -> Self {
        match value {
            wit::Value::Null => Self::Null,
            wit::Value::Integer(value) => Self::Integer(value),
            wit::Value::Real(value) => Self::Real(value),
            wit::Value::Text(value) => Self::Text(value),
            wit::Value::Blob(value) => Self::Blob(value),
        }
    }
}

impl From<&Value> for wit::Value {
    fn from(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Integer(value) => Self::Integer(*value),
            Value::Real(value) => Self::Real(*value),
            Value::Text(value) => Self::Text(value.clone()),
            Value::Blob(value) => Self::Blob(value.clone()),
        }
    }
}

/// A row of a query: its values in the order of the columns.
#[derive(Debug, Clone, PartialEq)]
pub struct Row(Vec<Value>);

impl Row {
    pub fn new(values: Vec<Value>) -> Self {
        Self(values)
    }

    pub fn get(&self, index: usize) -> Option<&Value> {
        self.0.get(index)
    }

    pub fn values(&self) -> &[Value] {
        &self.0
    }

    pub fn into_values(self) -> Vec<Value> {
        self.0
    }
}

impl From<wit::Row> for Row {
    fn from(row: wit::Row) -> Self {
        Self(row.values.into_iter().map(Into::into).collect())
    }
}

/// An error of the database: of SQLite, or a refusal of the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    /// Whether the lapp has no `database` permission.
    pub fn is_permission_denied(&self) -> bool {
        self.0.starts_with("permission denied")
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

fn wit_params(params: &[Value]) -> Vec<wit::Value> {
    params.iter().map(Into::into).collect()
}

/// Runs one statement with the parameters `?1`, `?2`, …; the number of the changed rows.
pub fn execute(sql: &str, params: &[Value]) -> Result<u64> {
    wit::execute(sql, &wit_params(params)).map_err(Error)
}

/// Runs one query; all of its rows.
pub fn query(sql: &str, params: &[Value]) -> Result<Vec<Row>> {
    wit::query(sql, &wit_params(params))
        .map(|rows| rows.into_iter().map(Into::into).collect())
        .map_err(Error)
}

/// Runs one query; its first row, if any.
pub fn query_row(sql: &str, params: &[Value]) -> Result<Option<Row>> {
    wit::query_row(sql, &wit_params(params))
        .map(|row| row.map(Into::into))
        .map_err(Error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_from_rust_types() {
        assert_eq!(Value::from(7), Value::Integer(7));
        assert_eq!(Value::from(true), Value::Integer(1));
        assert_eq!(Value::from("a"), Value::Text("a".into()));
        assert_eq!(Value::from(None::<i64>), Value::Null);
        assert_eq!(Value::from(Some(1.5)), Value::Real(1.5));
        assert_eq!(Value::from(vec![1u8]).as_blob(), Some(&[1u8][..]));
    }

    #[test]
    fn values_to_wit_and_back() {
        let values = [
            Value::Null,
            Value::Integer(-1),
            Value::Real(0.5),
            Value::Text("t".into()),
            Value::Blob(vec![9]),
        ];
        let back: Vec<Value> = wit_params(&values).into_iter().map(Into::into).collect();
        assert_eq!(back, values);
    }

    #[test]
    fn permission_denied_is_told_apart() {
        assert!(Error("permission denied: database".into()).is_permission_denied());
        assert!(!Error("no such table: tasks".into()).is_permission_denied());
    }
}
