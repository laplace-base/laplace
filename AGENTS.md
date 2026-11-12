# CLAUDE.md

Guidance for AI agents when working in this repository.

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
| `crates/settings` | `laplace-settings` | `Laplace.toml` (platform), lapp `manifest.toml` and user decision schemas, shared by host and front; file IO behind the `io` feature |
| `crates/common-api` | `laplace-common-api` | Serde-able `http::{Request,Response,Header}` shared by host and front |
| `crates/front-api` | `laplace-front-api` | wasm-side bindings to `window.__TAURI__.core.invoke` |
| `examples/echo/{back,front}` | `echo-back`, `echo-front` | Reference lapp |
| `wit/http.wit` | — | The WIT world every lapp back implements |
| `webassets/vendor` | — | Vendored htmx + wingy CSS, deployed as the `vendor` "app" |
| `crates/app/gen/android` | — | Generated-but-committed Android Studio project (`cargo tauri android init`); `app/src/main/assets/data/` is what the app unpacks at startup |
| `lapps/` | — | Gitignored. **Build output**: `lapps/back/<name>/{manifest.toml,<name>-back.wasm}`, `lapps/front/<name>/`. **Runtime, user-owned, never touched by `cargo make`**: `lapps/config/<name>.toml`, `lapps/data/<name>/` (see Configuration) |
| `docs/` | — | Architecture, domain glossary and requirements (Russian); read before design work |
| `issues/` | — | Task files (Russian): open ones at the top level, `done/`, `deferred/`; `26-10-01 migration plan.md` is the roadmap of porting the old Laplace (`../old/laplace`) features |

`crates/app` builds both a lib (`laplace_app_lib`) and a bin; `main.rs` only parses CLI args,
loads settings and logging, then calls `laplace_app_lib::run_desktop`.

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

cargo make android   # build + sign + install + launch the APK on a connected device
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
Both `front` and `back` also run `deploy-manifest`, which copies the lapp manifest
`examples/<APP_NAME>/manifest.toml` to `lapps/back/<APP_NAME>/manifest.toml` — every lapp
has that directory, with a backend or not.

CI (`.github/workflows/ci.yml`) runs only `cargo make lint` and `cargo make test`.

Host tests live in `crates/app/tests/` and run real lapp backends through
`laplace_app_lib::api::http::process_http` — the `handle_http` path without Tauri. `tests/lapps.rs`
builds temporary lapp roots (`Fixture`) with the manifests and user settings a test needs and the
built echo component copied in as each lapp's backend, so `cargo make test` first builds and deploys
the components (`test-backs`); a new backend a test needs goes there too. Plain `cargo test` fails on
a missing component.

**Agents: run builds quietly.** A full build prints hundreds of `Compiling …` lines that only waste
context, so suppress the progress output and keep only warnings, errors and test results:

```sh
CARGO_TERM_QUIET=true cargo make lint
CARGO_TERM_QUIET=true cargo make test
cargo check -q --message-format=short -p laplace-app   # direct cargo: also one line per diagnostic
```

`CARGO_TERM_QUIET` hides cargo's status lines; warnings, errors and test results still show, plus a
few `[cargo-make] INFO` lines per task. Do **not** add `--loglevel error` to `cargo make`: it
suppresses the output of the commands themselves, compiler errors included, and leaves only the exit
code. Only when a diagnostic needs its full context, rerun that one crate without
`--message-format=short`; pipe long outputs through `tail`/`grep` rather than reading them whole.

## Android

`cargo tauri android` additionally needs `ANDROID_HOME`, `NDK_HOME` (pointing at an **installed** NDK
version) and `JAVA_HOME`; `zipalign`/`apksigner` come from the SDK build-tools. The `android-*` tasks
in `Makefile.toml` wrap init / build / keystore / sign / install / start / logcat; `cargo make
android` chains them, and `-p debug` switches to a debuggable APK signed with the debug key (the
signing task then skips itself).

A mobile package has no filesystem to read lapps from, so everything the desktop app reads next to
the binary is shipped inside the APK as Android assets and unpacked at startup:

- the `android-assets` task stages `lapps/front/` and `lapps/back/` into
  `crates/app/gen/android/app/src/main/assets/data/` (gitignored) next to the committed
  `data/Laplace.toml` — the platform config of the package;
- `run_mobile` (`crates/app/src/lib.rs`) calls `android::prepare_data_dir`
  (`crates/app/src/android.rs`), which picks the data directory and fills it from the `data` assets
  through JNI;
- the data directory is `Documents/Laplace` on the shared storage — it outlives the package, is
  writable from outside and shows up over MTP (the unpacked files are handed to
  `MediaScannerConnection`, without which `MediaStore` does not index them). Reaching it by path
  needs the all-files access, so when the app has none it opens the settings screen that grants it
  and falls back for that run to `/sdcard/Android/data/org.laplace/files/data`, which needs no
  permission at all;
- from there on the mobile path is the desktop one: `Laplace.toml` is read from the data directory
  and its relative paths are joined with it, the lapps are registered by `LappsManager::load`
  and the frontends are served by `FsStoredAssets` through the lapp schemes.

Unpacking never destroys what is in the data directory, because the user owns it. It happens on the
first start after the package has been installed or updated and then never again
(`PackageInfo.lastUpdateTime`, remembered in `.unpacked`): a missing file is written, an existing one
is replaced only if `platform.mobile.overwrite_existing_assets` is set, and nothing is deleted. Every
other start unpacks nothing whatsoever and runs on what is in the directory, so a file the user has
deleted does not come back and their edits and their own lapps are left alone. `Laplace.toml` is the
exception on both counts: it is put back whenever it is missing, since nothing can start without it,
and it is never replaced. It also governs the unpacking, so `android::unpack_settings` restores and
reads it before the rest of the data is written.

A window is an activity. `tao` starts one per `WebviewWindowBuilder`, looking the class up by the
name passed to `WebviewWindowBuilder::activity_name` (Android only), so every activity a window can
be must exist as a Kotlin class in `org.laplace` *and* be declared in the manifest. There are two:
`MainActivity` — the launcher window, the `LAUNCHER` entry point — and `LappActivity`, which
`open_lapp` (`crates/app/src/api/laplace.rs`) names for every lapp it opens. Both are empty
subclasses of `LaplaceActivity`, which is where the shared window setup lives: the webview is laid
out edge to edge (Android 15+ gives no choice) and the content view is padded by the system-bar,
cutout and IME insets, so the page does not run under the status bar or the keyboard.

`LappActivity` is declared with `documentLaunchMode="always"`, which puts every instance of it in a
task of its own. That is what makes each open lapp a separate entry of the recents screen instead of
a stack on top of the launcher; the launcher window keeps running in the main task, and so do the
lapp windows, since all of them are webviews of one process and one event loop.
`autoRemoveFromRecents` drops the entry when its window is closed, and `maxRecents="50"` is the
system maximum — below it, opening one lapp too many drops the task of the least recently used one.
Nothing is per lapp: the user adds and removes them, so one activity class serves them all and the
manifest never has to know their number.

The release build is minified (`isMinifyEnabled` in `app/build.gradle.kts`), and R8 sees no caller
for what only native code calls by name — it drops such a method and the JNI call then fails with
`NoSuchMethodError` in the release build alone, while `-p debug` works. Every Kotlin member the Rust
side reaches over JNI therefore needs a keep rule in `app/proguard-rules.pro`; R8 reports what it has
dropped in `app/build/outputs/mapping/universalRelease/usage.txt`.

Opening a lapp that is already open raises its window instead of building a second one — the window
label is the lapp name, and `handle_http` routes by it, so a lapp has one window. Raising it is
platform work: `Window::set_focus` is a no-op in `tao` on Android, and a task is brought back by the
activity that sits in it, so `android::window::move_lapp_window_to_front` calls the static
`LappActivity.moveToFront` over JNI. The lapp windows register themselves there by the name `wry`
gives the webview — which `tauri-runtime-wry` sets to the window label — and the one that is asked
for moves its own task to the front through `ActivityManager.AppTask`.

The storage permissions live in `crates/app/gen/android/app/src/main/AndroidManifest.xml`:
`MANAGE_EXTERNAL_STORAGE` for Android 11+, `READ_EXTERNAL_STORAGE` / `WRITE_EXTERNAL_STORAGE` plus
`requestLegacyExternalStorage` for the API levels below it. The all-files access is a *special*
access: it is never granted by a request dialog, only by the user in settings, and it has to be
re-checked on every start. Granting it from a shell is
`adb shell appops set --uid org.laplace MANAGE_EXTERNAL_STORAGE allow`.

`android.rs` reaches the `AssetManager` through the activity `tao` registers
(`tao::platform::android::prelude::main_android_context`, re-exported by `tauri::tao`); the mobile
entry point runs on a thread spawned *before* that registration, hence the wait loop. `jni` is
pinned to the version `tao` links so that only one copy of it ends up in the binary.

Because of this, `lapps/front/` and `lapps/back/` must be built *before* the APK — the
`android-build` and `android-dev` tasks depend on `android-assets` for that. Logs go to logcat under
the `Laplace` tag (`android_logger`, initialized in `logging::init_mobile`); everything that happens
before the unpacked settings are read — the unpacking itself included — can only be seen under the
`RustStdoutStderr` tag, into which `tao` redirects stdout and stderr.

Only Android is wired up: `run_mobile` is `#[cfg(target_os = "android")]`, and an iOS package would
need its own way to unpack the bundle.

## Configuration

- `Laplace.toml` in the repo root (platform config, **gitignored** — it is a local file, not a template):
  `platform.lang`, `platform.frontend_assets_root` (defaults to `lapps/front`),
  `log.spec`, `lapps.back_root`, `lapps.config_root`, `lapps.data_root`, `lapps.allowed`. Path is
  overridable with `-c/--config`. Env overrides use the `LAPLACE__` prefix, e.g. `LAPLACE__PLATFORM__LANG=ru`.
- A lapp lives in four roots (`docs/architecture.md`, «Раскладка на диске»): the author's
  `lapps.back_root/<name>/` (`manifest.toml` + optional `<name>-back.wasm`) and
  `platform.frontend_assets_root/<name>/`, replaced by builds, unpacking and installs; the user's
  `lapps.config_root/<name>.toml` (decisions: enabled, autoload, granted permissions, data dir) and
  `lapps.data_root/<name>/` (default data dir), which none of those touch. A lapp name is
  `[a-z0-9-]` (`laplace_settings::lapp::is_valid_name`); `LappsManager::load` silently skips entries
  starting with `.` and logs an error for other invalid names.
- `examples/<name>/manifest.toml` — per-lapp manifest (`LappManifestSettings`): application metadata,
  `required_permissions.front` / `.back`, database, network, lapp-to-lapp requests. The host only
  ever reads it. The user decisions are `LappUserSettings` (`lapps.config_root/<name>.toml`); a
  missing file means the initial ones (`LappUserSettings::new`). Changes go through `UpdateQuery`;
  the effective permissions are `LappUserSettings::effective_permissions` (docs/domain.md).
  Reading and writing these files (`LappManifestSettings::load`, `LappUserSettings::{load, save, update}`)
  and `LaplaceSettings::new` sit behind the `io` feature of `laplace-settings`, which the host
  enables, so that the crate builds for `wasm32-unknown-unknown` without `config`.
  The host registry is `LappsManager` (`crates/app/src/lapps.rs`, docs/architecture.md «Реестр
  лаппов»): manifest + user settings for every lapp, backends started at startup for
  `enabled && autoload` and lazily otherwise, `enabled` / backend / `invoke_back` checked before every
  call. The launcher is not in it: it is part of the platform, the name `laplace` is only reserved.
- `crates/app/Tauri.toml` — Tauri config. It is **compiled into the binary** by
  `tauri::generate_context!()`, so changing it requires a rebuild of `laplace-app`. The assets it
  embeds from `frontend-dist` are never used: `start` replaces them with none (`assets::NoAssets`), so
  `tauri://` serves nothing. Every window, the launcher included, is on a URI scheme of its own,
  `lapp-<name>://localhost` (`assets::lapp_url`; the launcher URL in `Tauri.toml` is the same), which
  gives each lapp its own origin and web storage on every platform. `start` registers the scheme of
  every registered lapp and of the launcher; its handler (`FsStoredAssets::respond`) reads from
  `platform.frontend_assets_root` and answers only the window of that lapp, only `/<name>/**` and the
  shared `/vendor/**`, never outside of the root. A lapp registered after startup would have no scheme
  until a restart. A lapp window navigates only within its own frontend (`on_navigation`,
  `assets::is_navigation_allowed`) and opens no new windows (`on_new_window`). The names `laplace` and
  `vendor` are reserved (`lapp::is_reserved`).
- `crates/app/gen/android/app/src/main/assets/data/Laplace.toml` — the same platform config for the
  Android package, with paths relative to the data directory. It is unpacked once and never
  overwritten afterwards, so on a device the file the app reads is
  `Documents/Laplace/Laplace.toml`, which the user may edit. Its `platform.mobile` section is read
  on mobile only: `overwrite_existing_assets` (default `true`) lets the unpacking after an
  installation or update replace the data directory files that are already there; turned off, it
  only writes the ones missing from it.

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
- Comments and doc comments: only where they add something the code does not say, and as short as
  possible — usually one line. No restating the code, no history of the change, no essays on
  alternatives; design reasoning belongs in `docs/` or the issue file.

## Known rough edges

- `NOTES.md` (Russian, gitignored) tracks open design questions — mobile bootstrap flow, README
  structure.
- The lapp frontends are packed into the APK twice: `tauri::generate_context!` embeds
  `frontend-dist` (`lapps/front/`) into the binary even though it is never served,
  and `android-assets` ships the same files as package assets.
- `laplace-app` does not compile while `lapps/front/` is missing — `generate_context!` requires
  `frontend-dist` to exist; CI creates it empty.
- `crates/front/src/lapps.rs::list()` returns a hardcoded single `echo` entry: the host commands
  `get_lapps` and `update_lapp` exist, but the launcher does not call them yet (stage 6 of the plan).
- Commands are restricted by hand, not by the Tauri ACL: every platform management command lives in
  `crates/app/src/api/laplace.rs` and calls `require_launcher` first — the launcher window is the one
  labelled `laplace` in `Tauri.toml`. A new management command must do the same.
