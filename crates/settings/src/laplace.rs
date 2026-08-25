use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub use config::ConfigError;
use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct PlatformSettings {
    pub lang: String,
    #[serde(default = "default_frontend_assets_root")]
    pub frontend_assets_root: PathBuf,
}

impl Default for PlatformSettings {
    fn default() -> Self {
        Self {
            lang: "en".into(),
            frontend_assets_root: default_frontend_assets_root(),
        }
    }
}

fn default_frontend_assets_root() -> PathBuf {
    PathBuf::from("lapps").join("front")
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct P2pSettings {
    pub mdns_discovery_enabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct LoggerSettings {
    #[serde(default = "default_spec")]
    pub spec: String,

    pub path: Option<PathBuf>,

    pub duplicate_to_stdout: bool,

    #[serde(default = "default_keep_log_for_days")]
    pub keep_log_for_days: usize,
}

impl Default for LoggerSettings {
    fn default() -> Self {
        Self {
            spec: default_spec(),
            path: None,
            duplicate_to_stdout: false,
            keep_log_for_days: default_keep_log_for_days(),
        }
    }
}

fn default_spec() -> String {
    "info".into()
}

const fn default_keep_log_for_days() -> usize {
    7
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct LappsSettings {
    pub path: PathBuf,
    pub allowed: Option<HashSet<String>>,
}

impl Default for LappsSettings {
    fn default() -> Self {
        Self {
            path: "lapps".into(),
            allowed: None,
        }
    }
}

#[derive(Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct LaplaceSettings {
    pub platform: PlatformSettings,
    pub p2p: P2pSettings,
    pub log: LoggerSettings,
    pub lapps: LappsSettings,
}

impl LaplaceSettings {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::from(path.as_ref()))
            // Add in settings from the environment (with a prefix of LAPLACE)
            // Eg.. `LAPLACE_PLATFORM_LANG=ru laplace-app` would set the `platform.lang` key
            .add_source(
                Environment::with_prefix("LAPLACE")
                    .separator("_")
                    .with_list_parse_key("lapps.allowed")
                    .list_separator(",")
                    .try_parsing(true),
            )
            .build()?;
        config.try_deserialize()
    }
}
