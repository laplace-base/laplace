use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use std::{env, fs, process, thread};

use laplace_app_lib::api::http::{HttpError, process_http};
use laplace_app_lib::{LappError, LappsManager, State};
use laplace_common_api::http::{Request, StatusCode};
use laplace_settings::laplace::LappsSettings;
use laplace_settings::lapp::access::{FrontPermission, Permission};
use laplace_settings::lapp::update::UpdateQuery;
use laplace_settings::lapp::{LappUserSettings, MAIN_NAME, VENDOR_NAME};

const MANIFEST: &str = r#"
[application]
title = "Test"

[required.permissions]
front = ["invoke_back"]
"#;

/// Temporary lapp roots; every lapp with a backend runs the built echo component.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(test: &str) -> Self {
        let root = env::temp_dir().join(format!("laplace-lapps-{}-{test}", process::id()));
        fs::remove_dir_all(&root).ok();
        Self { root }
    }

    fn settings(&self) -> LappsSettings {
        LappsSettings {
            back_root: self.root.join("back"),
            config_root: self.root.join("config"),
            data_root: self.root.join("data"),
            call_timeout_ms: 500,
            memory_limit_mb: 64,
            ..Default::default()
        }
    }

    fn lapp(&self, name: &str, manifest: &str, back: bool, user_config: Option<&str>) -> &Self {
        self.lapp_with_back(name, manifest, back.then_some("echo"), user_config)
    }

    /// A lapp whose backend is the built component of the example `back`.
    fn lapp_with_back(&self, name: &str, manifest: &str, back: Option<&str>, user_config: Option<&str>) -> &Self {
        let settings = self.settings();
        let lapp_back_dir = settings.back_root.join(name);

        fs::create_dir_all(&lapp_back_dir).unwrap();
        fs::write(lapp_back_dir.join("manifest.toml"), manifest).unwrap();

        if let Some(back) = back {
            let built =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../lapps/back/{back}/{back}-back.wasm"));
            fs::copy(&built, lapp_back_dir.join(format!("{name}-back.wasm")))
                .unwrap_or_else(|err| panic!("'{back}' is not built, run `cargo make test`: {err}"));
        }

        if let Some(user_config) = user_config {
            fs::create_dir_all(&settings.config_root).unwrap();
            fs::write(settings.user_config_path(name), user_config).unwrap();
        }

        self
    }

    fn state(&self) -> State {
        State {
            lang: "en".into(),
            lapps: LappsManager::load(&self.settings()),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).ok();
    }
}

fn get(state: &State, lapp_name: &str) -> Result<String, HttpError> {
    let request = Request::get(format!("/{lapp_name}/api/hello")).build().unwrap();
    let response = process_http(state, lapp_name, request)?;

    assert_eq!(response.status, StatusCode::OK);

    Ok(String::from_utf8(response.body).unwrap())
}

#[test]
fn echo_responds_to_get() {
    let fixture = Fixture::new("echo");
    fixture.lapp("echo", MANIFEST, true, None);
    let state = fixture.state();

    assert_eq!(get(&state, "echo").unwrap(), "Echo :////echo/api/hello");
}

#[test]
fn request_to_missing_lapp_fails() {
    let fixture = Fixture::new("missing");
    let state = fixture.state();

    assert!(matches!(
        get(&state, "missing"),
        Err(HttpError::Lapp(LappError::NotFound(name))) if name == "missing"
    ));
}

#[test]
fn lapp_without_user_settings_has_the_initial_ones() {
    let fixture = Fixture::new("initial");
    fixture.lapp("fresh", MANIFEST, true, None);
    let state = fixture.state();

    let user = state.lapps.lapp("fresh").unwrap().user_settings_read();
    assert!(user.enabled());
    assert!(!user.autoload());
    assert!(!state.lapps.is_running("fresh"));
    assert!(get(&state, "fresh").is_ok());
    assert!(!fixture.settings().user_config_path("fresh").exists());
}

#[test]
fn disabled_lapp_fails() {
    let fixture = Fixture::new("disabled");
    fixture.lapp("off", MANIFEST, true, Some("[application]\nenabled = false\n"));
    let state = fixture.state();

    assert!(matches!(
        get(&state, "off"),
        Err(HttpError::Lapp(LappError::NotEnabled(name))) if name == "off"
    ));
    assert!(!state.lapps.is_running("off"));
}

#[test]
fn autoload_starts_at_load_and_the_rest_on_first_request() {
    let user_config = |autoload| {
        format!("[application]\nautoload = {autoload}\n\n[allowed.permissions]\nfront = [\"invoke_back\"]\n")
    };
    let fixture = Fixture::new("autoload");
    fixture.lapp("eager", MANIFEST, true, Some(&user_config(true))).lapp(
        "lazy",
        MANIFEST,
        true,
        Some(&user_config(false)),
    );
    let state = fixture.state();

    assert!(state.lapps.is_running("eager"));
    assert!(!state.lapps.is_running("lazy"));

    assert!(get(&state, "lazy").is_ok());
    assert!(state.lapps.is_running("lazy"));
}

#[test]
fn lapp_without_back_fails() {
    let fixture = Fixture::new("no-back");
    fixture.lapp("front-only", MANIFEST, false, None);
    let state = fixture.state();

    assert!(matches!(
        get(&state, "front-only"),
        Err(HttpError::Lapp(LappError::NoBackend(name))) if name == "front-only"
    ));
}

#[test]
fn launcher_is_not_a_registered_lapp() {
    let fixture = Fixture::new("launcher");
    fixture.lapp(MAIN_NAME, MANIFEST, true, None);
    let state = fixture.state();

    assert!(state.lapps.lapp(MAIN_NAME).is_none());
    assert!(matches!(
        get(&state, MAIN_NAME),
        Err(HttpError::Lapp(LappError::NotFound(name))) if name == MAIN_NAME
    ));
}

#[test]
fn lapp_without_invoke_back_is_denied() {
    let fixture = Fixture::new("denied");
    fixture
        .lapp("revoked", MANIFEST, true, Some("[allowed.permissions]\nfront = []\n"))
        .lapp("unrequired", "[application]\ntitle = \"Test\"\n", true, None);
    let state = fixture.state();

    for lapp_name in ["revoked", "unrequired"] {
        let err = get(&state, lapp_name).unwrap_err();
        assert!(matches!(
            &err,
            HttpError::Lapp(LappError::PermissionDenied { name, permission: Permission::Front(FrontPermission::InvokeBack) })
                if name == lapp_name
        ));
        assert_eq!(
            err.to_string(),
            format!("Lapp '{lapp_name}' is denied the permission 'invoke_back'")
        );
        assert!(!state.lapps.is_running(lapp_name));
    }
}

#[test]
fn lapp_with_unreadable_settings_is_skipped() {
    let fixture = Fixture::new("unreadable");
    fixture
        .lapp("bad-manifest", "[application\n", true, None)
        .lapp("bad-user-config", MANIFEST, true, Some("enabled = "))
        .lapp("good", MANIFEST, true, None);
    let state = fixture.state();

    assert!(state.lapps.lapp("bad-manifest").is_none());
    assert!(state.lapps.lapp("bad-user-config").is_none());
    assert!(get(&state, "good").is_ok());
}

#[test]
fn get_lapps_lists_registered_lapps_by_name() {
    let fixture = Fixture::new("get-lapps");
    fixture
        .lapp("second", MANIFEST, true, Some("[application]\nautoload = true\n"))
        .lapp("first", MANIFEST, true, None);
    let state = fixture.state();

    let lapps = state.lapps.lapps();
    let names: Vec<_> = lapps.iter().map(|lapp| lapp.name.as_str()).collect();
    assert_eq!(names, ["first", "second"]);

    let (first, second) = (&lapps[0], &lapps[1]);
    assert_eq!(first.manifest_settings.title(), "Test");
    assert!(first.user_settings.enabled());
    assert_eq!(first.effective_permissions().front, [FrontPermission::InvokeBack]);
    assert!(!first.is_running);

    assert!(second.user_settings.autoload());
    assert!(second.effective_permissions().front.is_empty());
    assert!(second.is_running);
}

#[test]
fn disabling_stops_the_lapp() {
    let fixture = Fixture::new("disable");
    fixture.lapp("echo", MANIFEST, true, None);

    let state = fixture.state();
    let manifest_path = fixture.settings().manifest_path("echo");
    let config_path = fixture.settings().user_config_path("echo");

    assert!(get(&state, "echo").is_ok());
    assert!(state.lapps.is_running("echo"));

    let query = UpdateQuery::new("echo").enabled(false);
    assert_eq!(state.lapps.update_lapp(query.clone()).unwrap(), query);

    assert!(!state.lapps.is_running("echo"));
    assert_eq!(fs::read_to_string(&manifest_path).unwrap(), MANIFEST);
    let manifest = state.lapps.lapps().remove(0).manifest_settings;
    assert!(!LappUserSettings::load(&config_path, &manifest).unwrap().enabled());
    assert!(matches!(
        get(&state, "echo"),
        Err(HttpError::Lapp(LappError::NotEnabled(_)))
    ));
}

#[test]
fn revoking_invoke_back_stops_the_lapp() {
    let fixture = Fixture::new("revoke");
    fixture.lapp("echo", MANIFEST, true, None);
    let state = fixture.state();

    assert!(get(&state, "echo").is_ok());

    let query = UpdateQuery::new("echo").deny_permission(FrontPermission::InvokeBack);
    assert_eq!(state.lapps.update_lapp(query.clone()).unwrap(), query);

    assert!(!state.lapps.is_running("echo"));
    assert!(matches!(
        get(&state, "echo"),
        Err(HttpError::Lapp(LappError::PermissionDenied { .. }))
    ));
}

#[test]
fn update_without_changes_keeps_everything() {
    let fixture = Fixture::new("no-op");
    fixture.lapp("echo", MANIFEST, true, None);
    let state = fixture.state();

    assert!(get(&state, "echo").is_ok());

    let updated = state
        .lapps
        .update_lapp(
            UpdateQuery::new("echo")
                .enabled(true)
                .allow_permission(FrontPermission::InvokeBack),
        )
        .unwrap();
    assert!(!updated.is_applied());
    assert!(state.lapps.is_running("echo"));
    assert!(!fixture.settings().user_config_path("echo").exists());

    assert!(matches!(
        state.lapps.update_lapp(UpdateQuery::new("missing").enabled(false)),
        Err(LappError::NotFound(_))
    ));
}

#[test]
fn window_title_falls_back_to_the_manifest_and_the_name() {
    let fixture = Fixture::new("title");
    fixture
        .lapp("renamed", MANIFEST, true, Some("[application]\ntitle = \"Mine\"\n"))
        .lapp(
            "untitled-user",
            MANIFEST,
            true,
            Some("[application]\nautoload = false\n"),
        )
        .lapp("untitled", "[application]\n", true, None);
    let state = fixture.state();

    let title = |name| state.lapps.check_open(name).unwrap().title(name);
    assert_eq!(title("renamed"), "Mine");
    assert_eq!(title("untitled-user"), "Test");
    assert_eq!(title("untitled"), "untitled");
}

#[test]
fn vendor_is_not_a_registered_lapp() {
    let fixture = Fixture::new("vendor");
    fixture.lapp(VENDOR_NAME, MANIFEST, true, None);
    let state = fixture.state();

    assert!(state.lapps.lapp(VENDOR_NAME).is_none());
}

#[test]
fn restarted_backend_is_not_compiled_again() {
    let fixture = Fixture::new("compiled-once");
    fixture.lapp("echo", MANIFEST, true, None);
    let state = fixture.state();

    assert!(get(&state, "echo").is_ok());

    // Without the component file only the compiled backend can answer.
    fs::remove_file(fixture.settings().back_root.join("echo/echo-back.wasm")).unwrap();
    for enabled in [false, true] {
        state
            .lapps
            .update_lapp(UpdateQuery::new("echo").enabled(enabled))
            .unwrap();
    }
    assert!(!state.lapps.is_running("echo"));

    assert_eq!(get(&state, "echo").unwrap(), "Echo :////echo/api/hello");
    assert!(state.lapps.is_running("echo"));
}

fn request(state: &State, lapp_name: &str, path: &str) -> Result<String, HttpError> {
    let request = Request::get(format!("/{lapp_name}/api/{path}")).build().unwrap();
    let response = process_http(state, lapp_name, request)?;
    Ok(String::from_utf8(response.body).unwrap())
}

#[test]
fn endless_call_is_interrupted_and_the_lapp_recovers() {
    let fixture = Fixture::new("deadline");
    fixture
        .lapp_with_back("stress", MANIFEST, Some("stress"), None)
        .lapp("echo", MANIFEST, true, None);
    let state = Arc::new(fixture.state());

    // Compiled beforehand, so that only the call itself is timed.
    assert_eq!(request(&state, "stress", "ok").unwrap(), "ok");

    let looping = {
        let state = state.clone();
        thread::spawn(move || {
            let started = Instant::now();
            (request(&state, "stress", "loop"), started.elapsed())
        })
    };

    // Another lapp is not held up by the endless call.
    assert_eq!(get(&state, "echo").unwrap(), "Echo :////echo/api/hello");

    let (result, elapsed) = looping.join().unwrap();
    assert!(
        matches!(result, Err(HttpError::Lapp(LappError::Timeout { ref name, timeout_ms: 500 })) if name == "stress"),
        "{result:?}"
    );
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    assert!(!state.lapps.is_running("stress"));

    assert_eq!(request(&state, "stress", "ok").unwrap(), "ok");
}

#[test]
fn memory_beyond_the_limit_fails_the_call_only() {
    let fixture = Fixture::new("memory");
    fixture.lapp_with_back("stress", MANIFEST, Some("stress"), None);
    let state = fixture.state();

    assert!(matches!(
        request(&state, "stress", "alloc"),
        Err(HttpError::Lapp(LappError::Handle { .. }))
    ));
    assert!(!state.lapps.is_running("stress"));

    assert_eq!(request(&state, "stress", "ok").unwrap(), "ok");
}

const VERSIONED_MANIFEST: &str = r#"
[application]
title = "Test"
version = "0.2.0"

[required.permissions]
front = ["invoke_back"]
"#;

fn user_version(fixture: &Fixture, state: &State, name: &str) -> String {
    let manifest = state.lapps.lapps().remove(0).manifest_settings;
    LappUserSettings::load(fixture.settings().user_config_path(name), &manifest)
        .unwrap()
        .application
        .app
        .version
}

#[test]
fn init_gets_the_previous_version_once() {
    let fixture = Fixture::new("init-upgrade");
    fixture.lapp_with_back(
        "stress",
        VERSIONED_MANIFEST,
        Some("stress"),
        Some("[application]\nversion = \"0.1.0\"\n\n[allowed.permissions]\nfront = [\"invoke_back\"]\n"),
    );
    let state = fixture.state();

    assert_eq!(request(&state, "stress", "previous").unwrap(), "0.1.0");
    assert_eq!(user_version(&fixture, &state, "stress"), "0.2.0");

    state.lapps.restart_lapp("stress").unwrap();
    assert_eq!(request(&state, "stress", "previous").unwrap(), "none");
}

#[test]
fn init_of_a_new_lapp_gets_no_version() {
    let fixture = Fixture::new("init-new");
    fixture.lapp_with_back("stress", VERSIONED_MANIFEST, Some("stress"), None);
    let state = fixture.state();

    assert_eq!(request(&state, "stress", "previous").unwrap(), "none");
    assert_eq!(user_version(&fixture, &state, "stress"), "0.2.0");
}

#[test]
fn failed_init_keeps_the_lapp_stopped_and_the_version() {
    let fixture = Fixture::new("init-fail");
    fixture.lapp_with_back(
        "stress",
        VERSIONED_MANIFEST,
        Some("stress"),
        Some("[application]\nversion = \"fail\"\n\n[allowed.permissions]\nfront = [\"invoke_back\"]\n"),
    );
    let state = fixture.state();

    let result = request(&state, "stress", "previous");
    assert!(
        matches!(&result, Err(HttpError::Lapp(LappError::Init { name, message }))
            if name == "stress" && message == "init is told to fail"),
        "{result:?}"
    );
    assert!(!state.lapps.is_running("stress"));
    assert_eq!(user_version(&fixture, &state, "stress"), "fail");
}

/// The log records of the test process, caught instead of being written.
fn logged() -> &'static Mutex<Vec<(String, log::Level, String)>> {
    struct Catcher;

    impl log::Log for Catcher {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.target().starts_with("lapp::")
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                logged()
                    .lock()
                    .unwrap()
                    .push((record.target().to_owned(), record.level(), record.args().to_string()));
            }
        }

        fn flush(&self) {}
    }

    static LOGGED: OnceLock<Mutex<Vec<(String, log::Level, String)>>> = OnceLock::new();
    LOGGED.get_or_init(|| {
        log::set_boxed_logger(Box::new(Catcher)).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
        Mutex::new(Vec::new())
    })
}

fn logged_by(target: &str, text: &str) -> Vec<(log::Level, String)> {
    logged()
        .lock()
        .unwrap()
        .iter()
        .filter(|(record_target, _, message)| record_target == target && message.contains(text))
        .map(|(_, level, message)| (*level, message.clone()))
        .collect()
}

#[test]
fn backend_output_goes_to_the_log_line_by_line() {
    logged();
    let fixture = Fixture::new("output");
    fixture.lapp_with_back("stress", MANIFEST, Some("stress"), None);
    let state = fixture.state();

    assert_eq!(request(&state, "stress", "print").unwrap(), "ok");

    assert_eq!(logged_by("lapp::stress", "stress std"), [
        (log::Level::Info, "stress stdout line".to_owned()),
        (log::Level::Warn, "stress stderr line".to_owned()),
        (log::Level::Warn, "stress stderr tail".to_owned()),
    ]);
}

#[test]
fn backend_panic_message_goes_to_the_log() {
    logged();
    let fixture = Fixture::new("panic");
    fixture.lapp_with_back("stress", MANIFEST, Some("stress"), None);
    let state = fixture.state();

    assert!(request(&state, "stress", "panic").is_err());

    let records = logged_by("lapp::stress", "stress is told to panic");
    assert!(matches!(records.as_slice(), [(log::Level::Warn, _), ..]), "{records:?}");
}

#[test]
fn backend_of_another_contract_is_not_started() {
    let fixture = Fixture::new("contract");
    fixture.lapp("old", MANIFEST, false, None);

    // A component with the old handler export only; the host reads the text format too.
    let old = r#"(component
        (core module $m (func (export "handle")))
        (core instance $i (instantiate $m))
        (func $handle (canon lift (core func $i "handle")))
        (instance $handler (export "handle" (func $handle)))
        (export "laplace:http/by-client-handler" (instance $handler))
    )"#;
    fs::write(fixture.settings().back_root.join("old/old-back.wasm"), old).unwrap();
    let state = fixture.state();

    let result = get(&state, "old");
    assert!(
        matches!(&result, Err(HttpError::Lapp(LappError::Start { name, error }))
            if name == "old" && error.to_string().contains("'laplace:lapp/handler@0.1.0'")),
        "{result:?}"
    );
}

const FILES_MANIFEST: &str = r#"
[application]
title = "Test"

[required.permissions]
front = ["invoke_back"]
back = ["file_read", "file_write"]
"#;

fn files_user_config(back_permissions: &[&str], data_dir: Option<&str>) -> String {
    let data_dir_prop = data_dir.map(|dir| format!("data_dir = {dir:?}\n")).unwrap_or_default();
    format!(
        "[application]\n{data_dir_prop}\n[allowed.permissions]\nfront = [\"invoke_back\"]\nback = {back_permissions:?}\n"
    )
}

fn files_fixture(test: &str, back_permissions: &[&str], data_dir: Option<&str>) -> (Fixture, State) {
    let fixture = Fixture::new(test);
    fixture.lapp_with_back(
        "stress",
        FILES_MANIFEST,
        Some("stress"),
        Some(&files_user_config(back_permissions, data_dir)),
    );
    let state = fixture.state();
    (fixture, state)
}

#[test]
fn backend_without_file_permissions_has_no_files() {
    let (fixture, state) = files_fixture("files-none", &[], None);

    assert!(
        request(&state, "stress", "write/note.txt")
            .unwrap()
            .starts_with("error")
    );
    assert!(request(&state, "stress", "read/note.txt").unwrap().starts_with("error"));
    assert!(!fixture.settings().data_root.join("stress").exists());
}

#[test]
fn file_read_reads_but_does_not_write() {
    let (fixture, state) = files_fixture("files-read", &["file_read"], None);
    let data_dir = fixture.settings().data_root.join("stress");

    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("note.txt"), "prepared").unwrap();

    assert_eq!(request(&state, "stress", "read/note.txt").unwrap(), "prepared");
    assert!(request(&state, "stress", "write/new.txt").unwrap().starts_with("error"));
    assert!(
        request(&state, "stress", "write/note.txt")
            .unwrap()
            .starts_with("error")
    );
    assert!(!data_dir.join("new.txt").exists());
    assert_eq!(fs::read_to_string(data_dir.join("note.txt")).unwrap(), "prepared");
}

#[test]
fn file_write_writes_into_the_data_directory() {
    let (fixture, state) = files_fixture("files-write", &["file_write"], None);

    assert_eq!(request(&state, "stress", "write/note.txt").unwrap(), "ok");
    assert_eq!(request(&state, "stress", "read/note.txt").unwrap(), "written by stress");
    assert_eq!(
        fs::read_to_string(fixture.settings().data_root.join("stress/note.txt")).unwrap(),
        "written by stress"
    );
}

#[test]
fn user_data_dir_moves_the_files() {
    let own = env::temp_dir().join(format!("laplace-lapps-{}-own-data", process::id()));
    fs::remove_dir_all(&own).ok();

    let (fixture, state) = files_fixture("files-own", &["file_write"], Some(own.to_str().unwrap()));

    assert_eq!(request(&state, "stress", "write/note.txt").unwrap(), "ok");
    assert_eq!(fs::read_to_string(own.join("note.txt")).unwrap(), "written by stress");
    assert!(!fixture.settings().data_root.join("stress").exists());

    fs::remove_dir_all(&own).ok();
}

#[test]
fn backend_does_not_leave_its_data_directory() {
    let (fixture, state) = files_fixture("files-outside", &["file_read", "file_write"], None);
    let data_root = fixture.settings().data_root;

    fs::create_dir_all(&data_root).unwrap();
    fs::write(data_root.join("outside.txt"), "outside").unwrap();

    assert!(
        request(&state, "stress", "read/../outside.txt")
            .unwrap()
            .starts_with("error")
    );
    assert!(
        request(&state, "stress", "write/../escaped.txt")
            .unwrap()
            .starts_with("error")
    );
    assert!(!data_root.join("escaped.txt").exists());
}

#[cfg(unix)]
#[test]
fn symlink_out_of_the_data_directory_is_not_followed() {
    use std::os;

    let (fixture, state) = files_fixture("files-symlink", &["file_read"], None);
    let data_root = fixture.settings().data_root;

    fs::create_dir_all(data_root.join("stress")).unwrap();
    fs::write(data_root.join("outside.txt"), "outside").unwrap();
    os::unix::fs::symlink(data_root.join("outside.txt"), data_root.join("stress/link.txt")).unwrap();

    let read = request(&state, "stress", "read/link.txt").unwrap();
    assert!(read.starts_with("error"), "{read}");
}
