use std::borrow::Cow;
use std::fs::{self, DirEntry};
use std::io;
use std::path::{Component, Path, PathBuf};

use laplace_settings::lapp::VENDOR_NAME;
use tauri::utils::assets::{AssetKey, AssetsIter, CspHash};
use tauri::{Assets, Runtime};

/// Whether the window of the lapp `lapp_name` may load the asset at the URL `path`: its own
/// frontend or the shared `vendor` assets, and nothing outside of them.
pub fn is_asset_allowed(lapp_name: &str, path: &str) -> bool {
    // Decoded as Tauri does before it looks the asset up, so that `%2F` cannot hide a `..`.
    let path = percent_encoding::percent_decode_str(path).decode_utf8_lossy();
    let Some(path) = path.strip_prefix('/') else {
        return false;
    };
    let mut components = Path::new(path).components();
    let first = components.next();

    matches!(first, Some(Component::Normal(first)) if first == lapp_name || first == VENDOR_NAME)
        && components.all(is_normal_component)
}

/// The path of `key` inside `root`, if the key does not lead out of it.
fn asset_path(root: &Path, key: &str) -> Option<PathBuf> {
    let relative = Path::new(key.strip_prefix('/').unwrap_or(key));
    relative
        .components()
        .all(is_normal_component)
        .then(|| root.join(relative))
}

fn is_normal_component(component: Component) -> bool {
    matches!(component, Component::Normal(_))
}

pub struct FsStoredAssets {
    root: PathBuf,
}

impl FsStoredAssets {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl<R: Runtime> Assets<R> for FsStoredAssets {
    fn get(&self, key: &AssetKey) -> Option<Cow<'_, [u8]>> {
        let Some(path) = asset_path(&self.root, key.as_ref()) else {
            log::warn!("Asset '{}' is rejected: it leads out of the assets root", key.as_ref());
            return None;
        };

        if path.exists() {
            Some(Cow::Owned(
                std::fs::read(&path)
                    .inspect_err(|err| log::error!("Error while reading file '{}': {err}", path.display()))
                    .ok()?,
            ))
        } else {
            log::warn!(
                "Asset '{}' not found: filе '{}' is not exists",
                key.as_ref(),
                path.display()
            );
            None
        }
    }

    fn iter(&self) -> Box<AssetsIter<'_>> {
        let mut files = Vec::new();
        if let Err(err) = visit_dirs(self.root.as_path(), &mut files) {
            log::error!("Error while visit directory '{}': {err}", self.root.display());
        }

        Box::new(FsStoredAssetsIter::new(&self.root, files))
    }

    fn csp_hashes(&self, _html_path: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
        Box::new(std::iter::empty())
    }
}

pub struct FsStoredAssetsIter<'a> {
    root: &'a Path,
    files: Vec<DirEntry>,
    index: usize,
}

impl<'a> FsStoredAssetsIter<'a> {
    pub fn new(root: &'a Path, files: Vec<DirEntry>) -> Self {
        Self { root, files, index: 0 }
    }
}

impl<'a> Iterator for FsStoredAssetsIter<'a> {
    type Item = (Cow<'a, str>, Cow<'a, [u8]>);

    fn next(&mut self) -> Option<Self::Item> {
        self.files.get(self.index).map(|entry| {
            self.index += 1;
            let path = entry
                .path()
                .strip_prefix(self.root)
                .map(|path| path.to_path_buf())
                .unwrap_or_else(|err| {
                    let path = entry.path();
                    log::error!(
                        "Error while stripping path '{path}' with root '{root}': {err}",
                        path = path.display(),
                        root = self.root.display(),
                    );
                    path
                });
            (
                Cow::Owned(path.to_string_lossy().to_string()),
                Cow::Owned(
                    std::fs::read(entry.path())
                        .inspect_err(|err| log::error!("Error while reading file '{}': {err}", entry.path().display()))
                        .ok()
                        .unwrap_or_default(),
                ),
            )
        })
    }
}

fn visit_dirs(dir: &Path, entries: &mut Vec<DirEntry>) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, entries)?;
            } else {
                entries.push(entry);
            }
        }
    }
    Ok(())
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
            assert!(!is_asset_allowed("echo", path), "{path}");
        }
    }

    #[test]
    fn assets_do_not_lead_out_of_the_root() {
        let dir = temp_dir("not_lead_out");
        let assets_root = dir.join("front");
        let lapp_assets_root = assets_root.join("echo");

        fs::create_dir_all(&lapp_assets_root).unwrap();
        fs::write(lapp_assets_root.join("index.html"), "echo").unwrap();
        fs::write(dir.join("secret.toml"), "secret").unwrap();
        let assets = FsStoredAssets::new(&assets_root);

        let get = |key: &str| {
            <FsStoredAssets as Assets<tauri::Wry>>::get(&assets, &AssetKey::from(key)).map(|data| data.into_owned())
        };
        assert_eq!(get("/echo/index.html").as_deref(), Some(b"echo".as_slice()));
        assert_eq!(get("/../secret.toml"), None);
        assert_eq!(get("/echo/../../secret.toml"), None);

        fs::remove_dir_all(dir).unwrap();
    }
}
