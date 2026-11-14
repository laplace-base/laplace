use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::lapp::access::{BackPermission, FrontPermission, Permission};
use crate::lapp::update::UpdateQuery;

pub mod access;
#[cfg(feature = "io")]
pub mod file;
pub mod update;

/// File name of the lapp manifest in the lapp directory.
pub const MANIFEST_FILE_NAME: &str = "manifest.toml";

/// Name of the launcher, the lapp of the platform itself.
pub const MAIN_NAME: &str = "laplace";

pub fn is_main(name: &str) -> bool {
    name == MAIN_NAME
}

/// Name of the frontend assets shared by every window, `<frontend_assets_root>/vendor/`.
pub const VENDOR_NAME: &str = "vendor";

/// Whether `name` belongs to the platform and cannot name a lapp.
pub fn is_reserved(name: &str) -> bool {
    is_main(name) || name == VENDOR_NAME
}

/// Whether `name` can name a lapp: it is a directory and file name, a window label and a host name.
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ApplicationSettings {
    #[serde(default)]
    pub version: String,
    pub title: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ApplicationUserSettings {
    #[serde(flatten)]
    pub app: ApplicationSettings,
    pub enabled: bool,
    pub autoload: bool,
    pub access_token: Option<String>,
    /// Data directory of the lapp: absolute, or relative to its default `<data_root>/<name>/`.
    pub data_dir: Option<PathBuf>,
}

impl Default for ApplicationUserSettings {
    fn default() -> Self {
        Self {
            app: ApplicationSettings::default(),
            enabled: true,
            autoload: false,
            access_token: None,
            data_dir: None,
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PermissionsSettings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub front: Vec<FrontPermission>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub back: Vec<BackPermission>,
}

impl PermissionsSettings {
    pub fn contains(&self, permission: Permission) -> bool {
        match permission {
            Permission::Front(permission) => self.front.contains(&permission),
            Permission::Back(permission) => self.back.contains(&permission),
        }
    }

    pub const fn is_empty(&self) -> bool {
        let Self { front, back } = self;
        front.is_empty() && back.is_empty()
    }

    pub fn front_iter(&self) -> impl Iterator<Item = FrontPermission> + '_ {
        self.front.iter().copied()
    }

    pub fn back_iter(&self) -> impl Iterator<Item = BackPermission> + '_ {
        self.back.iter().copied()
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DatabaseSettings {
    pub path: Option<PathBuf>,
}

impl DatabaseSettings {
    pub const fn new() -> Self {
        Self { path: None }
    }

    pub fn path(&self) -> &Path {
        self.path.as_deref().unwrap_or_else(|| Path::new(""))
    }

    pub fn into_path(self) -> PathBuf {
        self.path.unwrap_or_default()
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct NetworkSettings {
    #[serde(default, skip_serializing_if = "HttpSettings::is_empty")]
    pub http: HttpSettings,

    #[serde(default, skip_serializing_if = "GossipsubSettings::is_empty")]
    pub gossipsub: GossipsubSettings,
}

impl NetworkSettings {
    pub const fn new() -> Self {
        Self {
            http: HttpSettings::new(),
            gossipsub: GossipsubSettings::new(),
        }
    }

    pub const fn is_empty(&self) -> bool {
        let Self { http, gossipsub } = self;
        http.is_empty() && gossipsub.is_empty()
    }
}

/// The external HTTP access the manifest asks for, separately for either half of the lapp. It only
/// comes into effect with the `http` permission of that half and for the hosts the user allows.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct HttpSettings {
    #[serde(default, skip_serializing_if = "FrontHttpSettings::is_empty")]
    pub front: FrontHttpSettings,

    #[serde(default, skip_serializing_if = "BackHttpSettings::is_empty")]
    pub back: BackHttpSettings,
}

impl HttpSettings {
    pub const fn new() -> Self {
        Self {
            front: FrontHttpSettings::new(),
            back: BackHttpSettings::new(),
        }
    }

    pub const fn is_empty(&self) -> bool {
        let Self { front, back } = self;
        front.is_empty() && back.is_empty()
    }
}

/// The frontend can be limited by hosts only: its requests are filtered by the CSP of its window.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct FrontHttpSettings {
    #[serde(default, skip_serializing_if = "HttpHosts::is_empty")]
    pub hosts: HttpHosts,
}

impl FrontHttpSettings {
    pub const fn new() -> Self {
        Self {
            hosts: HttpHosts::new(),
        }
    }

    pub const fn is_empty(&self) -> bool {
        let Self { hosts } = self;
        hosts.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackHttpSettings {
    #[serde(default, skip_serializing_if = "HttpHosts::is_empty")]
    pub hosts: HttpHosts,

    #[serde(default, skip_serializing_if = "HttpMethods::is_empty")]
    pub methods: HttpMethods,

    #[serde(
        default = "BackHttpSettings::default_timeout_ms",
        skip_serializing_if = "BackHttpSettings::is_timeout_default"
    )]
    pub timeout_ms: u64,
}

impl Default for BackHttpSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl BackHttpSettings {
    pub const fn new() -> Self {
        Self {
            hosts: HttpHosts::new(),
            methods: HttpMethods::new(),
            timeout_ms: Self::default_timeout_ms(),
        }
    }

    pub const fn is_empty(&self) -> bool {
        let Self {
            hosts,
            methods,
            timeout_ms,
        } = self;
        hosts.is_empty() && methods.is_empty() && Self::is_timeout_default(timeout_ms)
    }

    const fn is_timeout_default(timeout_ms: &u64) -> bool {
        *timeout_ms == Self::default_timeout_ms()
    }

    const fn default_timeout_ms() -> u64 {
        1000 * 10
    }
}

/// Hosts of external HTTP access separately for either half of the lapp: what the user allows of
/// the manifest, or what comes into effect.
#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct HttpHostsSettings {
    pub front: HttpHosts,
    pub back: HttpHosts,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum HttpMethods {
    #[serde(with = "all_str")]
    All,

    List(Vec<HttpMethod>),
}

impl Default for HttpMethods {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpMethods {
    pub const fn new() -> Self {
        Self::All
    }

    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::List(methods) if methods.is_empty())
    }
}

/// Hosts of external HTTP access: `"all"` or a list; empty by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HttpHosts {
    #[serde(with = "all_str")]
    All,

    List(Vec<String>),
}

impl Default for HttpHosts {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpHosts {
    pub const fn new() -> Self {
        Self::List(Vec::new())
    }

    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::List(hosts) if hosts.is_empty())
    }

    /// The hosts of `self` that `allowed`, a choice among them, lets through: `"all"` of it is all
    /// of `self`, a host of it that `self` does not have is none.
    pub fn allowed_by(&self, allowed: &Self) -> Self {
        match (self, allowed) {
            (Self::All, allowed) => allowed.clone(),
            (Self::List(hosts), Self::All) => Self::List(hosts.clone()),
            (Self::List(hosts), Self::List(allowed)) => {
                Self::List(hosts.iter().filter(|host| allowed.contains(host)).cloned().collect())
            },
        }
    }
}

mod all_str {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str("all")
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<(), D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.to_lowercase().as_str() == "all" {
            Ok(())
        } else {
            Err(D::Error::custom(format!("Unknown string value: {value}")))
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct GossipsubSettings {
    pub addr: String,
    pub dial_ports: Vec<u16>,
}

impl GossipsubSettings {
    pub const fn new() -> Self {
        Self {
            addr: String::new(),
            dial_ports: Vec::new(),
        }
    }

    pub const fn is_empty(&self) -> bool {
        let Self { addr, dial_ports } = self;
        addr.is_empty() && dial_ports.is_empty()
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LappIncomingRequestSettings {
    pub methods: HttpMethods,
    pub request: String,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LappOutgoingRequestSettings {
    pub methods: HttpMethods,
    pub request: String,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LappRequestsSettings {
    pub lapp_name: String,
    pub incoming: Option<Vec<LappIncomingRequestSettings>>,
    pub outgoing: Option<Vec<LappOutgoingRequestSettings>>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct LappSettings {
    #[serde(default, skip_serializing_if = "PermissionsSettings::is_empty")]
    pub permissions: PermissionsSettings,

    #[serde(default, skip_serializing_if = "NetworkSettings::is_empty")]
    pub network: NetworkSettings,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lapp_requests: Vec<LappRequestsSettings>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LappManifestSettings {
    #[serde(default)]
    pub lapp_name: String,
    pub application: ApplicationSettings,
    pub required: LappSettings,
    pub database: Option<DatabaseSettings>,
}

impl LappManifestSettings {
    #[inline]
    pub fn name(&self) -> &str {
        &self.lapp_name
    }

    #[inline]
    pub fn title(&self) -> &str {
        &self.application.title
    }

    pub fn is_lapp_startup_active(&self, user: &LappUserSettings) -> bool {
        !is_main(self.name()) && user.autoload() && user.enabled()
    }

    pub fn database(&self) -> &DatabaseSettings {
        static DEFAULT: DatabaseSettings = DatabaseSettings::new();

        self.database.as_ref().unwrap_or(&DEFAULT)
    }

    pub fn into_database(self) -> DatabaseSettings {
        self.database.unwrap_or_default()
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LappUserSettings {
    pub application: ApplicationUserSettings,
    pub allowed: LappSettings,
}

impl LappUserSettings {
    /// The decisions about a lapp the user has not decided anything about yet: it is enabled and,
    /// if it requires to, may call its own backend, but has no other permission.
    pub fn new(manifest: &LappManifestSettings) -> Self {
        let mut settings = Self::default();
        settings.application.app.title = manifest.application.title.clone();
        settings.application.app.description = manifest.application.description.clone();
        if manifest
            .required
            .permissions
            .front
            .contains(&FrontPermission::InvokeBack)
        {
            settings.allow_front(FrontPermission::InvokeBack);
        }
        settings
    }

    pub fn data_dir(&self, data_root: impl AsRef<Path>, lapp_name: &str) -> PathBuf {
        let Some(data_dir) = &self.application.data_dir else {
            return data_root.as_ref().join(lapp_name);
        };

        if data_dir.is_absolute() {
            data_dir.clone()
        } else {
            data_root.as_ref().join(lapp_name).join(data_dir)
        }
    }

    /// The permissions the lapp actually has: the required ones that are allowed or implicit.
    pub fn effective_permissions(&self, manifest: &LappManifestSettings) -> PermissionsSettings {
        let required = &manifest.required.permissions;
        PermissionsSettings {
            front: self
                .allowed
                .permissions
                .front_iter()
                .filter(|permission| permission.is_implicit() || required.front.contains(permission))
                .collect(),
            back: self
                .allowed
                .permissions
                .back_iter()
                .filter(|permission| permission.is_implicit() || required.back.contains(permission))
                .collect(),
        }
    }

    /// The hosts the lapp actually reaches over HTTP: those of the manifest that the user allows, for
    /// a half of the lapp that has the effective `http` permission.
    pub fn effective_http_hosts(&self, manifest: &LappManifestSettings) -> HttpHostsSettings {
        let permissions = self.effective_permissions(manifest);
        let requested = &manifest.required.network.http;
        let allowed = &self.allowed.network.http;
        let effective = |has_permission: bool, requested: &HttpHosts, allowed: &HttpHosts| {
            if has_permission {
                requested.allowed_by(allowed)
            } else {
                HttpHosts::new()
            }
        };

        HttpHostsSettings {
            front: effective(
                permissions.front.contains(&FrontPermission::Http),
                &requested.front.hosts,
                &allowed.front.hosts,
            ),
            back: effective(
                permissions.back.contains(&BackPermission::Http),
                &requested.back.hosts,
                &allowed.back.hosts,
            ),
        }
    }

    /// Applies `query` and returns the part of it that has changed something.
    ///
    /// A permission the manifest does not require is not allowed, unless it is implicit.
    pub fn apply(&mut self, mut query: UpdateQuery, manifest: &LappManifestSettings) -> UpdateQuery {
        if let Some(enabled) = query.enabled {
            if self.enabled() != enabled {
                self.set_enabled(enabled);
            } else {
                query.enabled = None;
            }
        }

        if let Some(autoload) = query.autoload {
            if self.autoload() != autoload {
                self.set_autoload(autoload);
            } else {
                query.autoload = None;
            }
        }

        if let Some(permission) = query.allow_permission
            && !((permission.is_implicit() || manifest.required.permissions.contains(permission))
                && self.allow(permission))
        {
            query.allow_permission = None;
        }

        if let Some(permission) = query.deny_permission
            && !self.deny(permission)
        {
            query.deny_permission = None;
        }

        query
    }

    pub fn is_allowed(&self, permission: Permission) -> bool {
        self.allowed.permissions.contains(permission)
    }

    pub fn allow(&mut self, permission: Permission) -> bool {
        match permission {
            Permission::Front(permission) => self.allow_front(permission),
            Permission::Back(permission) => self.allow_back(permission),
        }
    }

    pub fn deny(&mut self, permission: Permission) -> bool {
        match permission {
            Permission::Front(permission) => self.deny_front(permission),
            Permission::Back(permission) => self.deny_back(permission),
        }
    }

    #[inline]
    pub fn title(&self) -> &str {
        &self.application.app.title
    }

    #[inline]
    pub fn enabled(&self) -> bool {
        self.application.enabled
    }

    #[inline]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.application.enabled = enabled;
    }

    #[inline]
    pub fn switch_enabled(&mut self) {
        self.set_enabled(!self.enabled());
    }

    #[inline]
    pub fn autoload(&self) -> bool {
        self.application.autoload
    }

    #[inline]
    pub fn set_autoload(&mut self, autoload: bool) {
        self.application.autoload = autoload;
    }

    #[inline]
    pub fn switch_autoload(&mut self) {
        self.set_autoload(!self.autoload());
    }

    pub fn is_front_allowed(&self, permission: FrontPermission) -> bool {
        self.allowed.permissions.front.contains(&permission)
    }

    pub fn is_back_allowed(&self, permission: BackPermission) -> bool {
        self.allowed.permissions.back.contains(&permission)
    }

    pub fn allow_front(&mut self, permission: FrontPermission) -> bool {
        if !self.is_front_allowed(permission) {
            self.allowed.permissions.front.push(permission);
            true
        } else {
            false
        }
    }

    pub fn allow_back(&mut self, permission: BackPermission) -> bool {
        if !self.is_back_allowed(permission) {
            self.allowed.permissions.back.push(permission);
            true
        } else {
            false
        }
    }

    pub fn deny_front(&mut self, permission: FrontPermission) -> bool {
        let index = self
            .allowed
            .permissions
            .front
            .iter()
            .position(|allowed| *allowed == permission);
        if let Some(index) = index {
            self.allowed.permissions.front.remove(index);
            true
        } else {
            false
        }
    }

    pub fn deny_back(&mut self, permission: BackPermission) -> bool {
        let index = self
            .allowed
            .permissions
            .back
            .iter()
            .position(|allowed| *allowed == permission);
        if let Some(index) = index {
            self.allowed.permissions.back.remove(index);
            true
        } else {
            false
        }
    }
}

/// What the launcher shows of a registered lapp.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LappInfo {
    pub name: String,
    pub manifest_settings: LappManifestSettings,
    pub user_settings: LappUserSettings,
    pub is_running: bool,
}

impl LappInfo {
    pub fn effective_permissions(&self) -> PermissionsSettings {
        self.user_settings.effective_permissions(&self.manifest_settings)
    }

    pub fn effective_http_hosts(&self) -> HttpHostsSettings {
        self.user_settings.effective_http_hosts(&self.manifest_settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lapp_settings(front: &[FrontPermission], back: &[BackPermission]) -> LappManifestSettings {
        LappManifestSettings {
            lapp_name: "test".into(),
            application: ApplicationSettings {
                title: "Test".into(),
                description: Some("A test lapp".into()),
                ..Default::default()
            },
            required: LappSettings {
                permissions: PermissionsSettings {
                    front: front.to_vec(),
                    back: back.to_vec(),
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn hosts(hosts: &[&str]) -> HttpHosts {
        HttpHosts::List(hosts.iter().map(|host| host.to_string()).collect())
    }

    #[test]
    fn allowed_hosts_are_a_choice_among_the_requested() {
        let requested = hosts(&["a.org", "b.org"]);

        assert_eq!(requested.allowed_by(&HttpHosts::All), requested);
        assert_eq!(requested.allowed_by(&hosts(&["b.org", "c.org"])), hosts(&["b.org"]));
        assert_eq!(requested.allowed_by(&HttpHosts::new()), HttpHosts::new());
        assert_eq!(HttpHosts::All.allowed_by(&hosts(&["c.org"])), hosts(&["c.org"]));
        assert_eq!(HttpHosts::All.allowed_by(&HttpHosts::All), HttpHosts::All);
        assert!(HttpHosts::new().allowed_by(&HttpHosts::All).is_empty());
    }

    #[test]
    fn effective_http_hosts_need_the_permission_and_the_user_choice() {
        let mut manifest = lapp_settings(&[FrontPermission::Http], &[BackPermission::Http]);
        manifest.required.network = NetworkSettings {
            http: HttpSettings {
                front: FrontHttpSettings {
                    hosts: hosts(&["cdn.org", "api.org"]),
                },
                back: BackHttpSettings {
                    hosts: HttpHosts::All,
                    ..BackHttpSettings::new()
                },
            },
            ..NetworkSettings::new()
        };

        let mut user = LappUserSettings::new(&manifest);
        assert_eq!(user.effective_http_hosts(&manifest), HttpHostsSettings::default());

        user.allow_front(FrontPermission::Http);
        user.allowed.network.http = HttpSettings {
            front: FrontHttpSettings {
                hosts: hosts(&["api.org", "other.org"]),
            },
            back: BackHttpSettings {
                hosts: hosts(&["any.org"]),
                ..BackHttpSettings::new()
            },
        };
        assert_eq!(user.effective_http_hosts(&manifest), HttpHostsSettings {
            front: hosts(&["api.org"]),
            back: HttpHosts::new(),
        });

        user.allow_back(BackPermission::Http);
        assert_eq!(user.effective_http_hosts(&manifest).back, hosts(&["any.org"]));
    }

    #[test]
    fn initial_user_settings() {
        let user = LappUserSettings::new(&lapp_settings(&[FrontPermission::InvokeBack], &[
            BackPermission::Database,
        ]));
        assert!(user.enabled());
        assert!(!user.autoload());
        assert_eq!(user.title(), "Test");
        assert_eq!(user.application.app.description.as_deref(), Some("A test lapp"));
        assert_eq!(user.allowed.permissions, PermissionsSettings {
            front: vec![FrontPermission::InvokeBack],
            back: vec![],
        });

        let user = LappUserSettings::new(&lapp_settings(&[], &[]));
        assert_eq!(user.allowed.permissions, PermissionsSettings::default());
    }

    #[test]
    fn effective_permissions() {
        let manifest = lapp_settings(&[FrontPermission::InvokeBack], &[
            BackPermission::Database,
            BackPermission::Http,
            BackPermission::LappsIncoming,
        ]);
        let mut user = LappUserSettings::default();
        user.allow_front(FrontPermission::LappsOutgoing);
        user.allow_front(FrontPermission::Http);
        user.allow_back(BackPermission::Database);
        user.allow_back(BackPermission::Tcp);

        assert_eq!(user.effective_permissions(&manifest), PermissionsSettings {
            front: vec![FrontPermission::LappsOutgoing],
            back: vec![BackPermission::Database],
        });
    }

    #[test]
    fn apply_update() {
        let manifest = lapp_settings(&[FrontPermission::InvokeBack], &[BackPermission::Database]);
        let mut user = LappUserSettings::new(&manifest);

        let query = UpdateQuery::new("test")
            .enabled(true)
            .autoload(true)
            .allow_permission(BackPermission::Database)
            .deny_permission(FrontPermission::InvokeBack);
        assert_eq!(
            user.apply(query, &manifest),
            UpdateQuery::new("test")
                .autoload(true)
                .allow_permission(BackPermission::Database)
                .deny_permission(FrontPermission::InvokeBack)
        );
        assert!(user.autoload());
        assert!(user.is_back_allowed(BackPermission::Database));
        assert!(!user.is_front_allowed(FrontPermission::InvokeBack));

        let query = UpdateQuery::new("test")
            .allow_permission(BackPermission::Database)
            .deny_permission(BackPermission::Tcp);
        assert!(!user.apply(query, &manifest).is_applied());
    }

    #[test]
    fn allow_only_required_or_implicit() {
        let manifest = lapp_settings(&[], &[]);
        let mut user = LappUserSettings::new(&manifest);

        let updated = user.apply(
            UpdateQuery::new("test").allow_permission(BackPermission::Http),
            &manifest,
        );
        assert_eq!(updated.allow_permission, None);
        assert!(!user.is_back_allowed(BackPermission::Http));

        let updated = user.apply(
            UpdateQuery::new("test").allow_permission(BackPermission::LappsIncoming),
            &manifest,
        );
        assert_eq!(updated.allow_permission, Some(BackPermission::LappsIncoming.into()));
        assert!(user.is_back_allowed(BackPermission::LappsIncoming));
    }

    #[test]
    fn data_dir() {
        let data_root = Path::new("/lapps/data");
        let mut user = LappUserSettings::default();
        assert_eq!(user.data_dir(data_root, "notes"), Path::new("/lapps/data/notes"));

        user.application.data_dir = Some("/home/user/Notes".into());
        assert_eq!(user.data_dir(data_root, "notes"), Path::new("/home/user/Notes"));

        user.application.data_dir = Some("archive".into());
        assert_eq!(
            user.data_dir(data_root, "notes"),
            Path::new("/lapps/data/notes/archive")
        );
    }

    #[test]
    fn startup_active() {
        let mut user = LappUserSettings::default();
        user.set_autoload(true);
        assert!(lapp_settings(&[], &[]).is_lapp_startup_active(&user));

        let mut launcher = lapp_settings(&[], &[]);
        launcher.lapp_name = MAIN_NAME.into();
        assert!(!launcher.is_lapp_startup_active(&user));

        user.set_enabled(false);
        assert!(!lapp_settings(&[], &[]).is_lapp_startup_active(&user));
    }
}
