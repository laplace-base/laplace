use std::fs;
use std::path::PathBuf;

use anyhow::{Context as _, anyhow};
use laplace_settings::lapp::access::BackPermission;
use log::Level;
use wasmtime::StoreLimits;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use self::output::LogOutput;
use crate::api::http::laplace::lapp::database::{Host as DatabaseHost, Row, Value};
use crate::database::{self, CallDeadline, Database};

pub mod output;

/// What an instance of a lapp backend is given by the registry: the host does not read the lapp
/// settings while the instance runs, and a change of them restarts it.
#[derive(Debug, Clone)]
pub struct InstanceContext {
    /// The effective permissions of the backend.
    pub permissions: Vec<BackPermission>,
    /// The data directory of the lapp.
    pub data_dir: PathBuf,
    /// The database file of the lapp, inside its data directory.
    pub database_path: PathBuf,
}

impl InstanceContext {
    /// The access to the data directory the permissions give: none, read-only, or read-write —
    /// WASI has no write without read, so `file_write` reads too.
    pub fn data_dir_perms(&self) -> Option<FsPerms> {
        if self.permissions.contains(&BackPermission::FileWrite) {
            Some(FsPerms::ReadWrite)
        } else if self.permissions.contains(&BackPermission::FileRead) {
            Some(FsPerms::ReadOnly)
        } else {
            None
        }
    }
}

pub struct WasiStates {
    table: ResourceTable,
    ctx: WasiCtx,
    stdout: LogOutput,
    stderr: LogOutput,
    permissions: Vec<BackPermission>,
    database: Database,
    pub deadline: CallDeadline,
    pub limits: StoreLimits,
}

impl WasiStates {
    pub fn new(lapp_name: &str, context: &InstanceContext) -> anyhow::Result<Self> {
        // The stdout and stderr of the backend go into the host log.
        let stdout = LogOutput::new(lapp_name, Level::Info);
        let stderr = LogOutput::new(lapp_name, Level::Warn);

        let mut builder = WasiCtxBuilder::new();
        builder.stdout(stdout.clone()).stderr(stderr.clone());

        // The data directory is the root `/` of the backend if a file permission allows it, and is
        // created then; without one the backend has no directory at all.
        if let Some(perms) = context.data_dir_perms() {
            let data_dir = &context.data_dir;
            fs::create_dir_all(data_dir)
                .with_context(|| format!("Failed to create the data directory '{}'", data_dir.display()))?;
            builder
                .preopened_dir(data_dir, "/", perms)
                .map_err(|err| anyhow!("Failed to open the data directory '{}': {err:#}", data_dir.display()))?;
        }
        let ctx = builder.build();
        let deadline = CallDeadline::default();

        Ok(Self {
            table: ResourceTable::new(),
            ctx,
            stdout,
            stderr,
            permissions: context.permissions.clone(),
            database: Database::new(context.database_path.clone(), deadline.clone()),
            deadline,
            limits: StoreLimits::default(),
        })
    }

    /// Writes out what the backend has left without a line break, so that it does not wait for the
    /// next call.
    pub fn flush_output(&self) {
        self.stdout.flush();
        self.stderr.flush();
    }
}

impl WasiStates {
    /// Refuses a host function without its permission.
    fn check(&self, permission: BackPermission) -> Result<(), String> {
        if self.permissions.contains(&permission) {
            Ok(())
        } else {
            Err(format!("permission denied: {}", permission.as_str()))
        }
    }

    fn database(&mut self) -> Result<&rusqlite::Connection, String> {
        self.check(BackPermission::Database)?;
        self.database.connection()
    }
}

impl DatabaseHost for WasiStates {
    fn execute(&mut self, sql: String, params: Vec<Value>) -> Result<u64, String> {
        database::execute(self.database()?, &sql, params).map_err(|err| err.to_string())
    }

    fn query(&mut self, sql: String, params: Vec<Value>) -> Result<Vec<Row>, String> {
        database::query(self.database()?, &sql, params).map_err(|err| err.to_string())
    }

    fn query_row(&mut self, sql: String, params: Vec<Value>) -> Result<Option<Row>, String> {
        database::query_row(self.database()?, &sql, params).map_err(|err| err.to_string())
    }
}

impl WasiView for WasiStates {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}
