use log::Level;
use wasmtime::StoreLimits;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use self::output::LogOutput;

pub mod output;

pub struct WasiStates {
    table: ResourceTable,
    ctx: WasiCtx,
    stdout: LogOutput,
    stderr: LogOutput,
    pub limits: StoreLimits,
}

impl WasiStates {
    /// The stdout and stderr of the backend go into the host log, target `lapp::<lapp_name>`.
    pub fn new(lapp_name: &str) -> Self {
        let stdout = LogOutput::new(lapp_name, Level::Info);
        let stderr = LogOutput::new(lapp_name, Level::Warn);
        let ctx = WasiCtxBuilder::new()
            .stdout(stdout.clone())
            .stderr(stderr.clone())
            .build();

        Self {
            table: ResourceTable::new(),
            ctx,
            stdout,
            stderr,
            limits: StoreLimits::default(),
        }
    }

    /// Writes out what the backend has left without a line break, so that it does not wait for the
    /// next call.
    pub fn flush_output(&self) {
        self.stdout.flush();
        self.stderr.flush();
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
