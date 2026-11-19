//! The SQLite database of a lapp backend: the host side of `laplace:lapp/database` (`wit/database.wit`).

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use anyhow::bail;
use rusqlite::Connection;
use rusqlite::types::{Value as SqlValue, ValueRef};

use crate::api::http::laplace::lapp::database::{Row, Value};

/// The file of the database in the data directory when the manifest names none.
pub const DEFAULT_FILE_NAME: &str = "database.sqlite";

/// How many SQLite VM instructions run between the checks of the call deadline.
const PROGRESS_OPS: i32 = 1000;

/// The database file of a lapp: `path` of its `[database]` manifest section — a relative path
/// inside the data directory, without `..` — or [`DEFAULT_FILE_NAME`].
pub fn database_path(data_dir: &Path, path: &Path) -> anyhow::Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Ok(data_dir.join(DEFAULT_FILE_NAME));
    }

    if !path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        bail!(
            "The database path '{}' is not a relative path inside the data directory",
            path.display()
        );
    }

    Ok(data_dir.join(path))
}

/// The deadline of the current call of the backend, which a long SQLite statement is interrupted at:
/// the epoch deadline does not interrupt a host call.
#[derive(Debug, Clone, Default)]
pub struct CallDeadline(Arc<Mutex<Option<Instant>>>);

impl CallDeadline {
    pub fn set(&self, deadline: Instant) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(deadline);
    }

    fn is_over(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some_and(|deadline| Instant::now() >= deadline)
    }
}

/// The database of an instance, opened on its first use.
pub struct Database {
    path: PathBuf,
    deadline: CallDeadline,
    connection: Option<Connection>,
}

impl Database {
    pub fn new(path: PathBuf, deadline: CallDeadline) -> Self {
        Self {
            path,
            deadline,
            connection: None,
        }
    }

    pub fn connection(&mut self) -> Result<&Connection, String> {
        if self.connection.is_none() {
            self.connection = Some(self.open().map_err(|err| format!("{err:#}"))?);
        }
        Ok(self.connection.as_ref().expect("the connection is open"))
    }

    fn open(&self) -> anyhow::Result<Connection> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }

        let mut connection = Connection::open(&self.path)?;
        let deadline = self.deadline.clone();
        connection.progress_handler(PROGRESS_OPS, Some(move || deadline.is_over()))?;

        Ok(connection)
    }
}

pub fn execute(connection: &Connection, sql: &str, params: Vec<Value>) -> rusqlite::Result<u64> {
    let changed = connection.execute(sql, rusqlite::params_from_iter(params.into_iter().map(into_sql)))?;
    Ok(changed as u64)
}

pub fn query(connection: &Connection, sql: &str, params: Vec<Value>) -> rusqlite::Result<Vec<Row>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(params.into_iter().map(into_sql)), to_row)?;
    rows.collect()
}

pub fn query_row(connection: &Connection, sql: &str, params: Vec<Value>) -> rusqlite::Result<Option<Row>> {
    let mut statement = connection.prepare(sql)?;
    let mut rows = statement.query(rusqlite::params_from_iter(params.into_iter().map(into_sql)))?;
    rows.next()?.map(to_row).transpose()
}

fn into_sql(value: Value) -> SqlValue {
    match value {
        Value::Null => SqlValue::Null,
        Value::Integer(value) => SqlValue::Integer(value),
        Value::Real(value) => SqlValue::Real(value),
        Value::Text(value) => SqlValue::Text(value),
        Value::Blob(value) => SqlValue::Blob(value),
    }
}

fn from_sql(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => Value::Integer(value),
        ValueRef::Real(value) => Value::Real(value),
        ValueRef::Text(value) => Value::Text(String::from_utf8_lossy(value).into()),
        ValueRef::Blob(value) => Value::Blob(value.into()),
    }
}

fn to_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    let values = (0..row.as_ref().column_count())
        .map(|index| row.get_ref(index).map(from_sql))
        .collect::<rusqlite::Result<_>>()?;
    Ok(Row { values })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn all_values() -> Vec<Value> {
        vec![
            Value::Null,
            Value::Integer(-42),
            Value::Real(1.5),
            Value::Text("текст".into()),
            Value::Blob(vec![0, 1, 255]),
        ]
    }

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        execute(&connection, "CREATE TABLE t(n, i, r, s, b)", Vec::new()).unwrap();
        connection
    }

    #[test]
    fn every_value_goes_there_and_back() {
        let connection = connection();
        let inserted = execute(&connection, "INSERT INTO t VALUES (?1, ?2, ?3, ?4, ?5)", all_values()).unwrap();
        assert_eq!(inserted, 1);

        let row = query_row(&connection, "SELECT * FROM t", Vec::new()).unwrap().unwrap();
        assert_eq!(format!("{:?}", row.values), format!("{:?}", all_values()));
    }

    #[test]
    fn query_returns_all_rows_and_query_row_none_for_no_rows() {
        let connection = connection();
        for i in 0..3 {
            execute(&connection, "INSERT INTO t(i) VALUES (?1)", vec![Value::Integer(i)]).unwrap();
        }

        let rows = query(&connection, "SELECT i FROM t WHERE i > ?1 ORDER BY i", vec![
            Value::Integer(0),
        ])
        .unwrap();
        assert_eq!(
            format!("{rows:?}"),
            format!("{:?}", [
                Row {
                    values: vec![Value::Integer(1)]
                },
                Row {
                    values: vec![Value::Integer(2)]
                }
            ])
        );
        assert!(
            query_row(&connection, "SELECT * FROM t WHERE i = 9", Vec::new())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn database_path_stays_inside_the_data_directory() {
        let data_dir = Path::new("/data/notes");

        assert_eq!(
            database_path(data_dir, Path::new("")).unwrap(),
            data_dir.join(DEFAULT_FILE_NAME)
        );
        assert_eq!(
            database_path(data_dir, Path::new("db/notes.db")).unwrap(),
            data_dir.join("db/notes.db")
        );
        for path in ["../x.db", "db/../../x.db", "/x.db", "./x.db"] {
            assert!(database_path(data_dir, Path::new(path)).is_err(), "{path}");
        }
    }

    #[test]
    fn long_statement_is_interrupted_at_the_deadline() {
        let deadline = CallDeadline::default();
        let mut database = Database::new(PathBuf::from(":memory:"), deadline.clone());
        deadline.set(Instant::now() + Duration::from_millis(100));

        let started = Instant::now();
        let endless = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c";
        let result = query_row(database.connection().unwrap(), endless, Vec::new());

        assert!(result.is_err(), "{result:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
