use std::fs;
use std::path::PathBuf;

use laplace_app_lib::api::http::{HttpError, process_http};
use laplace_app_lib::{LappError, LappsManager, State};
use laplace_common_api::http::{Request, StatusCode};
use laplace_settings::laplace::LappsSettings;
use laplace_settings::lapp::access::{FrontPermission, Permission};
use laplace_settings::lapp::update::UpdateQuery;
use laplace_settings::lapp::{LappUserSettings, MAIN_NAME};

const MANIFEST: &str = r#"
[application]
title = "Test"

[required_permissions]
front = ["invoke_back"]
"#;

/// Temporary lapp roots; every lapp with a backend runs the built echo component.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(test: &str) -> Self {
        let root = std::env::temp_dir().join(format!("laplace-lapps-{}-{test}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        Self { root }
    }

    fn settings(&self) -> LappsSettings {
        LappsSettings {
            back_root: self.root.join("back"),
            config_root: self.root.join("config"),
            data_root: self.root.join("data"),
            allowed: None,
        }
    }

    fn lapp(&self, name: &str, manifest: &str, back: bool, user_config: Option<&str>) -> &Self {
        let settings = self.settings();
        let lapp_back_dir = settings.back_root.join(name);

        fs::create_dir_all(&lapp_back_dir).unwrap();
        fs::write(lapp_back_dir.join("manifest.toml"), manifest).unwrap();

        if back {
            let echo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lapps/back/echo/echo-back.wasm");
            fs::copy(&echo, lapp_back_dir.join(format!("{name}-back.wasm")))
                .expect("Echo is not built, run `cargo make test`");
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
        format!("[application]\nautoload = {autoload}\n\n[allowed_permissions]\nfront = [\"invoke_back\"]\n")
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
        .lapp("revoked", MANIFEST, true, Some("[allowed_permissions]\nfront = []\n"))
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
    assert_eq!(first.manifest.title(), "Test");
    assert!(first.user_settings.enabled());
    assert_eq!(first.effective_permissions.front, [FrontPermission::InvokeBack]);
    assert!(!first.is_running);

    assert!(second.user_settings.autoload());
    assert!(second.effective_permissions.front.is_empty());
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
    let manifest = state.lapps.lapps().remove(0).manifest;
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
