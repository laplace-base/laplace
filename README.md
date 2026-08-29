# Laplace

The local-first web-application platform for the decentralized web.

## Building

All builds go through [`cargo-make`](https://github.com/sagiegurari/cargo-make); the tasks are
defined in [`Makefile.toml`](Makefile.toml).

### Prerequisites

```sh
rustup toolchain install nightly            # `cargo fmt` uses unstable options
rustup target add wasm32-unknown-unknown    # lapp frontends
rustup target add wasm32-wasip1             # lapp backends
cargo install cargo-make cargo-component wasm-bindgen-cli
cargo install tauri-cli --version "^2"      # provides `cargo tauri`
```

`wasm-opt` from [binaryen](https://github.com/WebAssembly/binaryen) must be on `PATH` for release
builds, and `watchexec` for `cargo make watch`. Building the desktop app also needs the usual Tauri
system dependencies (on Debian/Ubuntu: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`,
`librsvg2-dev`, `patchelf`).

### Desktop

```sh
cargo make all      # build the app, the Laplace frontend and the example lapps
cargo make run      # build everything and run the app
cargo make start    # run the already built app
```

Every task builds in release mode by default; pass `-p debug` right after `cargo make` for a debug
build, e.g. `cargo make -p debug run`.

The build deploys into `lapps/`: lapp frontends into `lapps/front/<name>/` and lapp backends into
`lapps/back/<name>/<name>-back.wasm`. On desktop the app reads both at runtime, from the paths
configured in `Laplace.toml`.

### Android

Additional prerequisites — the Android SDK with **build-tools**, **platform-tools** and an **NDK**,
plus a JDK 17 or newer:

```sh
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64
export ANDROID_HOME="$HOME/Android/Sdk"
export NDK_HOME="$ANDROID_HOME/ndk/<version>"   # must point at an installed NDK
export PATH="$PATH:$ANDROID_HOME/platform-tools"
```

`cargo tauri android init` installs the Rust Android targets itself; to add them by hand:

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

Then:

```sh
cargo make android              # build, sign, install and launch on the connected device
cargo make -p debug android     # the same with a debuggable build signed by the debug key
cargo make android-log          # follow the app log from the device
```

`cargo make android` chains the individual tasks, each of which can also be run on its own:

| Task | What it does |
| --- | --- |
| `android-assets` | stages the built lapps into the package assets; `android-build` and `android-dev` depend on it |
| `android-build` | `cargo tauri android build --apk` for all four ABIs |
| `android-keystore` | `keytool -genkeypair` into `android-release-key.jks` (skipped if it exists, and for `-p debug`) |
| `android-sign` | `zipalign` + `apksigner sign` of the release APK (skipped for `-p debug`) |
| `android-install` | `adb install -r` of the resulting APK |
| `android-start` | `adb shell am start` of the main activity |
| `android-dev` | `cargo tauri android dev` — debug build installed and run on a device with its log attached |

To create keystore export the needed environment variables in the `.env` file:

```sh
ANDROID_KEYSTORE=android-release-key.jks
ANDROID_KEYSTORE_PASS=…
ANDROID_KEY_PASS=…
```

Debug and release APKs are signed with different keys, so switching between them on the same device
needs an `adb uninstall org.laplace` in between. The app logs to logcat under the `Laplace` tag —
that is what `cargo make android-log` follows; the startup that precedes the log setup, unpacking
the assets included, shows up there under `RustStdoutStderr`.

An APK has no directory to read the lapps from, so `Laplace.toml`, `lapps/front/` and `lapps/back/`
are packed into it as assets (`android-assets` stages them into
`crates/app/gen/android/app/src/main/assets/data/`) and unpacked at startup into
`Documents/Laplace` on the shared storage:

```sh
adb shell ls /sdcard/Documents/Laplace
```

That directory is deliberately outside the app sandbox: it survives an uninstall, is writable from
a file manager and shows up over MTP, which is how lapps get added to the device. Reaching it needs
the all-files access, which only the user can grant — the app opens the settings screen for it on a
start that has no access and uses `/sdcard/Android/data/org.laplace/files/data` until then. From a
shell the access is granted with:

```sh
adb shell appops set --uid org.laplace MANAGE_EXTERNAL_STORAGE allow
```

Unpacking does not destroy anything in the data directory: missing files are written, existing ones
are refreshed only when the package has been reinstalled or updated, `Laplace.toml` is written once
and then belongs to the user, and files the user has added are never touched. The version in the
repository — the one the first start unpacks — is
`crates/app/gen/android/app/src/main/assets/data/Laplace.toml`; all the paths in it are relative to
the data directory.

APKs are written to `crates/app/gen/android/app/build/outputs/apk/universal/{release,debug}/`.
`crates/app/gen/android` is generated, but committed: edit it when the app needs Android-specific
manifest entries, permissions or Kotlin code.

## License

Licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE)
  or [apache.org/licenses/LICENSE-2.0](https://www.apache.org/licenses/LICENSE-2.0))
- MIT license ([LICENSE-MIT](LICENSE-MIT) or [opensource.org/licenses/MIT](https://opensource.org/licenses/MIT))

at your option.
