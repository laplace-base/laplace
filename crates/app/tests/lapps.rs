use std::fs;
use std::path::PathBuf;

use laplace_app_lib::api::http::process_http;
use laplace_app_lib::{LappsManager, State};
use laplace_common_api::http::{Request, StatusCode};
use laplace_settings::laplace::LappsSettings;
use laplace_settings::lapp::MAIN_NAME;

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
            path: self.root.join("back"),
            config_root: self.root.join("config"),
            data_root: self.root.join("data"),
            allowed: None,
        }
    }

    fn lapp(&self, name: &str, manifest: &str, back: bool, user_config: Option<&str>) -> &Self {
        let settings = self.settings();
        let lapp_back_dir = settings.path.join(name);

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

fn get(state: &State, lapp_name: &str) -> Result<String, String> {
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

    assert_eq!(get(&state, "missing").unwrap_err(), "Lapp 'missing' is not found");
}

#[test]
fn lapp_without_user_settings_has_the_initial_ones() {
    let fixture = Fixture::new("initial");
    fixture.lapp("fresh", MANIFEST, true, None);
    let state = fixture.state();

    let user = state.lapps.lapp("fresh").unwrap().user_settings();
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

    assert_eq!(get(&state, "off").unwrap_err(), "Lapp 'off' is not enabled");
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

    assert_eq!(
        get(&state, "front-only").unwrap_err(),
        "Lapp 'front-only' has no backend"
    );
}

#[test]
fn launcher_is_registered_without_back() {
    let fixture = Fixture::new("launcher");
    let state = fixture.state();

    assert!(state.lapps.lapp(MAIN_NAME).is_some());
    assert_eq!(get(&state, MAIN_NAME).unwrap_err(), "Lapp 'laplace' has no backend");
}

#[test]
fn lapp_without_invoke_back_is_denied() {
    let fixture = Fixture::new("denied");
    fixture
        .lapp("revoked", MANIFEST, true, Some("[allowed_permissions]\nfront = []\n"))
        .lapp("unrequired", "[application]\ntitle = \"Test\"\n", true, None);
    let state = fixture.state();

    for name in ["revoked", "unrequired"] {
        assert_eq!(
            get(&state, name).unwrap_err(),
            format!("Lapp '{name}' is denied the permission 'invoke_back'")
        );
        assert!(!state.lapps.is_running(name));
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
