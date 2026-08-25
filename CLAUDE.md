# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

Laplace — a local-first web-application platform. A Tauri 2 desktop shell (`laplace-app`)
hosts sandboxed applications called **lapps**. Each lapp has two halves:

- **back** — a WASI component (`wasm32-wasip1`, built with `cargo component`) run by
  `wasmtime` inside the host process. It exports the `laplace:http` WIT world (`wit/http.wit`).
- **front** — a `wasm32-unknown-unknown` `wasm-bindgen` module rendering HTML with
  `hypertext` + `wingy-hypertext`, driven client-side by htmx.

The frontend never talks HTTP over a socket. It builds a `laplace_common_api::http::Request`,
calls the Tauri command `handle_http`, and the host converts it to WIT types and invokes the
lapp's `by-client-handler.handle`. The Tauri window *label* is the lapp name — that is how
`handle_http` (`crates/app/src/api/http.rs`) picks which lapp receives the request.

## Workspace layout

| Path | Crate | Role |
| --- | --- | --- |
| `crates/app` | `laplace-app` | Tauri host: CLI, settings load, wasmtime lapp manager, Tauri commands, asset provider |
| `crates/front` | `laplace-front` | The Laplace launcher UI, compiled to wasm |
| `crates/settings` | `laplace-settings` | `Laplace.toml` (platform) and lapp `config.toml` schemas |
| `crates/common-api` | `laplace-common-api` | Serde-able `http::{Request,Response,Header}` shared by host and front |
| `crates/front-api` | `laplace-front-api` | wasm-side bindings to `window.__TAURI__.core.invoke` |
| `examples/echo/{back,front}` | `echo-back`, `echo-front` | Reference lapp |
| `wit/http.wit` | — | The WIT world every lapp back implements |
| `webassets/vendor` | — | Vendored htmx + wingy CSS, deployed as the `vendor` "app" |
| `lapps/` | — | **Build output** (gitignored): `lapps/back/<name>/<name>-back.wasm`, `lapps/front/<name>/` |

`crates/app` builds both a lib (`laplace_app_lib`) and a bin; `main.rs` only parses CLI args,
loads settings and logging, then calls `laplace_app_lib::run`.

## Building and running

Everything goes through **`cargo-make`**, not bare `cargo`. Required tooling:
`cargo-make`, `cargo-component`, `cargo-tauri`, `wasm-bindgen-cli`, `wasm-opt` (binaryen),
`watchexec` (for `watch`), and a **nightly** toolchain for `cargo fmt` (`.rustfmt.toml` uses
unstable options).

```sh
cargo make all       # vendor assets + laplace front & app + all examples
cargo make laplace   # launcher front (wasm) + Tauri app
cargo make echo      # echo lapp: front (wasm) + back (component)
cargo make run       # build all, then run target/release/laplace-app
cargo make start     # run the already-built binary
cargo make watch     # rebuild the front on change
cargo make lint      # nightly fmt --check + clippy -D warnings
cargo make test
```

Add `-p debug` for debug profiles: `cargo make -p debug run`. Without it everything builds
release / `wasm-release` (the `wasm-release` profile is `release` + `panic=abort` + `opt-level=z`
+ `wasm-opt -Oz`).

`RUSTFLAGS` is set globally in `Makefile.toml` to `--cfg=web_sys_unstable_apis` — required by
`wingy-hypertext-web` (Web Animations bindings). Building a wasm crate with plain `cargo build`
without that cfg will fail to link.

The pipeline per app (`APP_NAME`) is: `build-front` → `wasm-bindgen` into `target/dist` →
`wasm-opt` → copy `webassets/**` and `target/dist/*` into `lapps/front/<APP_NAME>/`.
Backs go `cargo component build` → copy to `lapps/back/<APP_NAME>/<APP_NAME>-back.wasm`.

CI (`.github/workflows/ci.yml`) runs only `cargo make lint` and `cargo make test`.

## Configuration

- `Laplace.toml` (platform config, **gitignored** — it is a local file, not a template):
  `platform.lang`, `platform.frontend_assets_root` (defaults to `lapps/front`),
  `log.spec`, `lapps.path`, `lapps.allowed`. Path is overridable with `-c/--config`.
  Env overrides use the `LAPLACE_` prefix, e.g. `LAPLACE_PLATFORM_LANG=ru`.
- `examples/<name>/config.toml` — per-lapp manifest (`LappSettings`): application metadata,
  `permissions.front` / `permissions.back`, database, network, lapp-to-lapp requests.
  Note: this manifest is defined in `crates/settings` but is **not yet read** by the host —
  `LappsManager::load_lapps` currently just scans `lapps.path` for directories.
- `crates/app/Tauri.toml` — Tauri config. It is **compiled into the binary** by
  `tauri::generate_context!()`, so changing it requires a rebuild of `laplace-app`.
  Assets are *not* embedded: `run()` replaces them with `FsStoredAssets`, which reads from
  `platform.frontend_assets_root` at runtime.

## Frontend conventions

`main.js` for each app: `init()` the wasm module, register the htmx `client-router` extension,
render the shell with `render_root()`, insert it into `#root`, `htmx.process`, then `wasm.init()`.

`define_client_router_ext(htmx, client_api, server_api, router, preproc)` intercepts
`htmx:configRequest`: requests matching `client_api` (or *not* matching `server_api`) are
handed to the Rust `router`/`route` function instead of hitting the network. The launcher passes
`(null, "/api")` — everything is client-routed; echo passes `("/echo/api", null)` — only its API
prefix is.

UI is written with `hypertext`'s `rsx!` macro plus `wingy-hypertext` components and class
constants. Several dependencies (`hypertext`, `iconic`, `wingy-hypertext*`) are pinned to git
branches in the workspace `Cargo.toml`.

## Style

- Rust 2024 edition, `resolver = "3"`.
- Formatting: `max_width = 120`, `group_imports = "StdExternalCrate"`,
  `imports_granularity = "Module"`, trailing comma after match blocks. **Nightly rustfmt only.**
- TOML: `taplo` per `.taplo.toml` (4-space indent, 120 cols, indented tables, dependency keys
  sorted alphabetically — keep `[workspace.dependencies]` sorted when adding entries).
- All workspace crates use `x = { workspace = true }` dependency inheritance; add new shared
  deps to `[workspace.dependencies]` first.

## Known rough edges

- `NOTES.md` (Russian, gitignored) tracks open design questions — mobile bootstrap flow, README
  structure, and the fact that the `app://` URI-scheme handler in `crates/app/src/lib.rs` is dead
  code (no window uses that scheme; it also reads paths from the filesystem root). Do not treat
  it as a working asset path.
- `crates/front/src/lapps.rs::list()` returns a hardcoded single `echo` entry; there is no
  command yet to enumerate real lapps from the host.
- `handle_http` and `LappState::new` use `expect`/panic liberally — a missing or broken lapp
  brings down the command.
