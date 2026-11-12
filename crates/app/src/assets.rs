//! The frontend assets of the windows.
//!
//! Every lapp, the launcher included, is served by a URI scheme of its own, `lapp-<name>://localhost`:
//! that gives each its own origin, and so its own web storage, on every platform — on Windows and
//! Android a custom scheme becomes `http://<scheme>.localhost`, so the scheme is the only part of the
//! URL that can tell the lapps apart. The built-in `tauri://` scheme serves nothing ([`NoAssets`]).

use std::borrow::Cow;
use std::fs;
use std::path::{Component, Path, PathBuf};

use laplace_settings::lapp::VENDOR_NAME;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::utils::assets::{AssetKey, AssetsIter, CspHash};
use tauri::utils::mime_type::MimeType;
use tauri::{Assets, Runtime, Url};

/// The URI scheme that serves the frontend of the lapp `lapp_name`.
pub fn lapp_scheme(lapp_name: &str) -> String {
    // A scheme has to start with a letter, a lapp name may start with a digit.
    format!("lapp-{lapp_name}")
}

/// The URL of the start page of the lapp `lapp_name`.
pub fn lapp_url(lapp_name: &str) -> Url {
    Url::parse(&format!(
        "{}://localhost/{lapp_name}/index.html",
        lapp_scheme(lapp_name)
    ))
    .expect("a lapp name makes a valid URL")
}

/// Whether the window of the lapp `lapp_name` may load the asset at the decoded URL `path`: its own
/// frontend or the shared `vendor` assets, and nothing outside of them.
fn is_asset_allowed(lapp_name: &str, path: &str) -> bool {
    path_top_dir(path).is_some_and(|top| top == lapp_name || top == VENDOR_NAME)
}

/// The first directory of the decoded URL `path`, if the path is made of plain components only.
fn path_top_dir(path: &str) -> Option<&str> {
    let mut components = Path::new(path.strip_prefix('/')?).components();
    let Some(Component::Normal(top)) = components.next() else {
        return None;
    };
    components.all(is_component_normal).then_some(top.to_str()?)
}

/// Whether the window of the lapp `lapp_name` may navigate to `url`: only to the pages of its own
/// frontend, on its own scheme — the custom form or the `http(s)://<scheme>.localhost` one of
/// Windows and Android.
pub fn is_navigation_allowed(lapp_name: &str, url: &Url) -> bool {
    let scheme = lapp_scheme(lapp_name);
    let host = url.host_str();
    let own_origin = (url.scheme() == scheme && host == Some("localhost"))
        || (matches!(url.scheme(), "http" | "https") && host == Some(&format!("{scheme}.localhost")));

    own_origin && path_top_dir(&decode(url.path())) == Some(lapp_name)
}

fn is_component_normal(component: Component) -> bool {
    matches!(component, Component::Normal(_))
}

fn decode(path: &str) -> Cow<'_, str> {
    percent_encoding::percent_decode_str(path).decode_utf8_lossy()
}

/// The frontend files in `platform.frontend_assets_root`.
pub struct FsStoredAssets {
    root: PathBuf,
}

impl FsStoredAssets {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The response of the scheme of the lapp `lapp_name` to a request of the window `webview_label`.
    ///
    /// Only the window of the lapp itself gets anything, and only what [`is_asset_allowed`] lets it.
    pub fn respond(&self, lapp_name: &str, webview_label: &str, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
        let path = request.uri().path();
        let decoded_path = decode(path);
        if webview_label != lapp_name || !is_asset_allowed(lapp_name, &decoded_path) {
            log::warn!("Window '{webview_label}' is denied the asset '{}'", request.uri());
            return status(StatusCode::FORBIDDEN);
        }

        let Some(file_path) = self.file_path(&decoded_path) else {
            return status(StatusCode::FORBIDDEN);
        };

        match fs::read(&file_path) {
            Ok(content) => Response::builder()
                .header(header::CONTENT_TYPE, MimeType::parse(&content, path))
                .body(content)
                .expect("the response is valid"),
            Err(err) => {
                log::warn!("Asset '{path}' is not read from '{}': {err}", file_path.display());
                status(StatusCode::NOT_FOUND)
            },
        }
    }

    /// The file of the URL `path`, if the path does not lead out of the root.
    fn file_path(&self, path: &str) -> Option<PathBuf> {
        let relative = Path::new(path.strip_prefix('/').unwrap_or(path));
        relative
            .components()
            .all(is_component_normal)
            .then(|| self.root.join(relative))
    }
}

fn status(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .body(Vec::new())
        .expect("the response is valid")
}

/// The assets of the built-in `tauri://` scheme: none, since every window is on its lapp scheme.
pub struct NoAssets;

impl<R: Runtime> Assets<R> for NoAssets {
    fn get(&self, key: &AssetKey) -> Option<Cow<'_, [u8]>> {
        log::warn!("Asset '{}' is requested through the tauri scheme", key.as_ref());
        None
    }

    fn iter(&self) -> Box<AssetsIter<'_>> {
        Box::new(std::iter::empty())
    }

    fn csp_hashes(&self, _html_path: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
        Box::new(std::iter::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("laplace-assets-{}-{name}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn lapp_window_gets_its_own_and_vendor_assets_only() {
        for lapp_name in ["echo", "notes"] {
            assert!(is_asset_allowed(lapp_name, "/vendor/htmx/2.0.8/htmx.min.js"));
        }
        assert!(is_asset_allowed("echo", "/echo/index.html"));
        assert!(is_asset_allowed("echo", "/echo/dist/echo-front.js"));

        for path in [
            "/laplace/index.html",
            "/other/index.html",
            "/vendor/..%2Fother/index.html",
            "/echo/..%2F..%2Ffile",
            "/echo/../other/index.html",
            "/",
            "",
            "/echo%2F..%2F..%2FLaplace.toml",
        ] {
            assert!(!is_asset_allowed("echo", &decode(path)), "{path}");
        }
    }

    #[test]
    fn lapp_window_navigates_within_its_own_frontend_only() {
        let allowed = |url: &str| is_navigation_allowed("echo", &Url::parse(url).unwrap());

        assert!(allowed("lapp-echo://localhost/echo/index.html"));
        assert!(allowed("lapp-echo://localhost/echo/sub/page.html?x=1#y"));
        assert!(allowed("http://lapp-echo.localhost/echo/index.html"));
        assert!(allowed("https://lapp-echo.localhost/echo/index.html"));
        assert!(allowed(lapp_url("echo").as_str()));

        for url in [
            "lapp-echo://localhost/laplace/index.html",
            "lapp-echo://localhost/vendor/htmx/2.0.8/htmx.min.js",
            "lapp-echo://localhost/echo/..%2F..%2FLaplace.toml",
            "lapp-echo://other/echo/index.html",
            "lapp-notes://localhost/echo/index.html",
            "lapp-laplace://localhost/laplace/index.html",
            "http://lapp-notes.localhost/echo/index.html",
            "http://localhost/echo/index.html",
            "https://example.org/echo/index.html",
            "tauri://localhost/echo/index.html",
            "about:blank",
        ] {
            assert!(!allowed(url), "{url}");
        }
    }

    #[test]
    fn lapp_url_is_on_its_own_scheme() {
        assert_eq!(lapp_url("echo").as_str(), "lapp-echo://localhost/echo/index.html");
        assert_eq!(lapp_url("2048").scheme(), "lapp-2048");
    }

    #[test]
    fn scheme_serves_only_the_window_of_its_lapp() {
        let dir = temp_dir("not_lead_out");
        let assets_root = dir.join("front");
        let lapp_assets_root = assets_root.join("echo");

        fs::create_dir_all(&lapp_assets_root).unwrap();
        fs::write(lapp_assets_root.join("index.html"), "echo").unwrap();
        fs::write(dir.join("secret.toml"), "secret").unwrap();
        let assets = FsStoredAssets::new(&assets_root);

        let get = |window: &str, uri: &str| {
            let request = Request::get(uri).body(Vec::new()).unwrap();
            assets.respond("echo", window, &request)
        };

        let response = get("echo", "lapp-echo://localhost/echo/index.html");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body(), b"echo");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "text/html");

        assert_eq!(
            get("notes", "lapp-echo://localhost/echo/index.html").status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            get("echo", "lapp-echo://localhost/..%2Fsecret.toml").status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            get("echo", "lapp-echo://localhost/echo/missing.js").status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(FsStoredAssets::new(&assets_root).file_path("/../secret.toml"), None);

        fs::remove_dir_all(dir).unwrap();
    }
}
