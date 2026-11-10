use std::path::Path;
use std::{fs, io};

use thiserror::Error;

use crate::lapp::update::UpdateQuery;
use crate::lapp::{LappSettings, LappUserSettings};

#[derive(Debug, Error)]
pub enum LappSettingsError {
    #[error("Settings file operation error: {0}")]
    Io(#[from] io::Error),

    #[error("Settings deserialization error: {0}")]
    Deserialize(#[from] toml::de::Error),

    #[error("Settings serialization error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub type LappSettingsResult<T> = Result<T, LappSettingsError>;

impl LappSettings {
    /// Reads the manifest of the lapp `lapp_name`; the host never writes it.
    pub fn load(lapp_name: impl Into<String>, path: impl AsRef<Path>) -> LappSettingsResult<Self> {
        let mut settings: Self = toml::from_str(&fs::read_to_string(path)?)?;
        settings.lapp_name = lapp_name.into();
        Ok(settings)
    }
}

impl LappUserSettings {
    /// Reads the user decisions about the lapp of `manifest`, or makes the initial ones if there
    /// is no file yet.
    pub fn load(path: impl AsRef<Path>, manifest: &LappSettings) -> LappSettingsResult<Self> {
        match fs::read_to_string(path) {
            Ok(content) => Ok(toml::from_str(&content)?),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::new(manifest)),
            Err(err) => Err(err.into()),
        }
    }

    /// Writes the settings atomically: to a temporary file next to `path`, then renames it.
    pub fn save(&self, path: impl AsRef<Path>) -> LappSettingsResult<()> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }

        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        let tmp_path = path.with_file_name(format!(".{file_name}.tmp"));
        fs::write(&tmp_path, toml::to_string(self)?)?;
        fs::rename(&tmp_path, path).inspect_err(|_| {
            fs::remove_file(&tmp_path).ok();
        })?;

        Ok(())
    }

    /// Applies `query` and saves the settings if it has changed anything.
    pub fn update(
        &mut self,
        query: UpdateQuery,
        manifest: &LappSettings,
        path: impl AsRef<Path>,
    ) -> LappSettingsResult<UpdateQuery> {
        let updated = self.apply(query, manifest);
        if updated.is_applied() {
            self.save(path)?;
        }
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::lapp::access::{BackPermission, FrontPermission};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("laplace-settings-{}-{name}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        dir
    }

    const MANIFEST: &str = r#"
        [application]
        version = "1.2.0"
        title = "Notes"
        description = "Notes in markdown"
        tags = ["notes"]

        [required_permissions]
        front = ["invoke_back"]
        back = ["file_read", "file_write"]

        [network.http]
        hosts = ["example.org"]
        timeout_ms = 500
    "#;

    fn write_manifest(dir: &Path) -> PathBuf {
        let path = dir.join("manifest.toml");
        fs::create_dir_all(dir).unwrap();
        fs::write(&path, MANIFEST).unwrap();
        path
    }

    #[test]
    fn load_manifest() {
        let dir = temp_dir("manifest");
        let manifest = LappSettings::load("notes", write_manifest(&dir)).unwrap();

        assert_eq!(manifest.name(), "notes");
        assert_eq!(manifest.application.version, "1.2.0");
        assert_eq!(manifest.title(), "Notes");
        assert_eq!(manifest.required_permissions.front, [FrontPermission::InvokeBack]);
        assert_eq!(manifest.required_permissions.back, [
            BackPermission::FileRead,
            BackPermission::FileWrite
        ]);
        assert_eq!(manifest.network().http().timeout_ms, 500);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_echo_manifest() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/echo/manifest.toml");
        let manifest = LappSettings::load("echo", path).unwrap();

        assert_eq!(manifest.title(), "Echo");
        assert_eq!(manifest.required_permissions.front, [FrontPermission::InvokeBack]);
    }

    #[test]
    fn load_missing_manifest_fails() {
        let dir = temp_dir("missing-manifest");

        assert!(matches!(
            LappSettings::load("notes", dir.join("manifest.toml")),
            Err(LappSettingsError::Io(_))
        ));
    }

    #[test]
    fn user_settings_round_trip() {
        let dir = temp_dir("user");
        let config_dir = dir.join("config");
        let manifest = LappSettings::load("notes", write_manifest(&dir)).unwrap();
        let config_path = config_dir.join("notes.toml");

        let mut user = LappUserSettings::load(&config_path, &manifest).unwrap();
        assert!(user.enabled());
        assert!(!user.autoload());
        assert_eq!(user.title(), "Notes");
        assert_eq!(user.allowed_permissions.front, [FrontPermission::InvokeBack]);
        assert!(!config_path.exists(), "loading must not create the file");

        user.application.data_dir = Some("/home/user/Notes".into());
        let updated = user
            .update(
                UpdateQuery::new("notes")
                    .autoload(true)
                    .allow_permission(BackPermission::FileRead)
                    .allow_permission(BackPermission::Database),
                &manifest,
                &config_path,
            )
            .unwrap();
        assert_eq!(updated, UpdateQuery::new("notes").autoload(true));
        assert!(config_path.exists());
        assert!(!config_dir.join(".notes.toml.tmp").exists());

        let loaded = LappUserSettings::load(&config_path, &manifest).unwrap();
        assert!(loaded.autoload());
        assert_eq!(loaded.title(), "Notes");
        assert_eq!(loaded.allowed_permissions.front, [FrontPermission::InvokeBack]);
        assert_eq!(
            loaded.application.data_dir.as_deref(),
            Some(Path::new("/home/user/Notes"))
        );

        let mut loaded = loaded;
        let updated = loaded
            .update(
                UpdateQuery::new("notes").allow_permission(BackPermission::FileRead),
                &manifest,
                &config_path,
            )
            .unwrap();
        assert_eq!(
            updated,
            UpdateQuery::new("notes").allow_permission(BackPermission::FileRead)
        );
        let loaded = LappUserSettings::load(&config_path, &manifest).unwrap();
        assert_eq!(loaded.allowed_permissions.back, [BackPermission::FileRead]);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn update_without_changes_does_not_write() {
        let dir = temp_dir("no-write");
        let manifest = LappSettings::load("notes", write_manifest(&dir)).unwrap();
        let config_path = dir.join("config").join("notes.toml");

        let mut user = LappUserSettings::load(&config_path, &manifest).unwrap();
        let updated = user
            .update(UpdateQuery::new("notes").enabled(true), &manifest, &config_path)
            .unwrap();
        assert!(!updated.is_applied());
        assert!(!config_path.exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn broken_user_settings_fail() {
        let dir = temp_dir("broken");
        let manifest = LappSettings::load("notes", write_manifest(&dir)).unwrap();
        let config_path = dir.join("notes.toml");

        fs::write(&config_path, "enabled = ").unwrap();

        assert!(matches!(
            LappUserSettings::load(&config_path, &manifest),
            Err(LappSettingsError::Deserialize(_))
        ));

        fs::remove_dir_all(dir).unwrap();
    }
}
