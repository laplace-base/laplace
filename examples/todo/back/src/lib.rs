//! The tasks of the Todo lapp in its database, relative to `/todo/api`:
//! - `GET /list` — all tasks;
//! - `POST /add` — a [`Task`] as JSON, its `id` is ignored;
//! - `POST /update` — a [`Task`] with its `id`;
//! - `POST /delete` — the `id` of a task;
//! - `POST /clear_completed`.
//!
//! Every one answers with [`Response`] as JSON: the tasks after the change, or the error.

use laplace_back_api::database::{self, Row, Value};
use laplace_back_api::http::{Request, Response as HttpResponse, StatusCode};
use laplace_back_api::{Lapp, Router};
use todo_common::{Response, Task};

struct Todo;

impl Lapp for Todo {
    /// Without the `database` permission the lapp still starts, and every request tells why it fails.
    fn init(_previous_version: Option<String>) -> Result<(), String> {
        let created = database::execute(
            "CREATE TABLE IF NOT EXISTS tasks(
                id INTEGER PRIMARY KEY,
                description TEXT NOT NULL,
                completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0, 1))
            )",
            &[],
        );
        match created {
            Err(err) if err.is_permission_denied() => {
                eprintln!("No tasks without the database: {err}");
                Ok(())
            },
            result => result.map(drop).map_err(|err| err.to_string()),
        }
    }

    fn handle(request: Request<Vec<u8>>) -> HttpResponse<Vec<u8>> {
        Router::new()
            .get("/list", |_| answer(list()))
            .post("/add", |request| answer(add(request.body())))
            .post("/update", |request| answer(update(request.body())))
            .post("/delete", |request| answer(delete(request.body())))
            .post("/clear_completed", |_| answer(clear_completed()))
            .handle(request)
    }
}

fn list() -> Result<Vec<Task>, String> {
    database::query("SELECT id, description, completed FROM tasks ORDER BY id", &[])
        .map_err(|err| err.to_string())?
        .into_iter()
        .map(task_from)
        .collect()
}

fn add(body: &[u8]) -> Result<Vec<Task>, String> {
    let task = parse::<Task>(body)?;
    execute("INSERT INTO tasks(description, completed) VALUES (?1, ?2)", &[
        task.description.into(),
        task.completed.into(),
    ])?;
    list()
}

fn update(body: &[u8]) -> Result<Vec<Task>, String> {
    let task = parse::<Task>(body)?;
    let changed = execute("UPDATE tasks SET description = ?1, completed = ?2 WHERE id = ?3", &[
        task.description.into(),
        task.completed.into(),
        task.id.into(),
    ])?;
    if changed == 0 {
        return Err(format!("No task {}", task.id));
    }
    list()
}

fn delete(body: &[u8]) -> Result<Vec<Task>, String> {
    let id = parse::<i64>(body)?;
    execute("DELETE FROM tasks WHERE id = ?1", &[id.into()])?;
    list()
}

fn clear_completed() -> Result<Vec<Task>, String> {
    execute("DELETE FROM tasks WHERE completed = 1", &[])?;
    list()
}

fn execute(sql: &str, params: &[Value]) -> Result<u64, String> {
    database::execute(sql, params).map_err(|err| err.to_string())
}

fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, String> {
    serde_json::from_slice(body).map_err(|err| format!("Invalid request: {err}"))
}

fn task_from(row: Row) -> Result<Task, String> {
    match row.values() {
        [Value::Integer(id), Value::Text(description), Value::Integer(completed)] => Ok(Task {
            id: *id,
            description: description.clone(),
            completed: *completed != 0,
        }),
        values => Err(format!("Invalid task row: {values:?}")),
    }
}

fn answer(result: Result<Vec<Task>, String>) -> HttpResponse<Vec<u8>> {
    let (status, response) = match result {
        Ok(tasks) => (StatusCode::OK, Response::List(tasks)),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Response::Error(err)),
    };
    let body = serde_json::to_vec(&response).expect("the response serializes");

    let mut response = HttpResponse::new(body);
    *response.status_mut() = status;
    response
}

laplace_back_api::export!(Todo);
