use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

pub struct WasiStates {
    table: ResourceTable,
    ctx: WasiCtx,
}

impl Default for WasiStates {
    fn default() -> Self {
        let table = ResourceTable::new();
        let ctx = WasiCtxBuilder::new().build();

        Self { table, ctx }
    }
}

impl WasiStates {
    pub fn new() -> Self {
        Self::default()
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
