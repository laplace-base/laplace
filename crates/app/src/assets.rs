use std::borrow::Cow;
use std::fs::{self, DirEntry};
use std::io;
use std::path::{Path, PathBuf};

use tauri::utils::assets::{AssetKey, AssetsIter, CspHash};
use tauri::{Assets, Runtime};

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
        let relative_path = key.as_ref().strip_prefix("/").unwrap_or(key.as_ref());
        let path = self.root.join(relative_path);
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
