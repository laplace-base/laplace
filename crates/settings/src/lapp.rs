use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
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
#[serde(default)]
pub struct PermissionsSettings {
    pub front: Vec<FrontPermission>,
    pub back: Vec<BackPermission>,
}

impl PermissionsSettings {
    pub fn contains(&self, permission: Permission) -> bool {
        match permission {
            Permission::Front(permission) => self.front.contains(&permission),
            Permission::Back(permission) => self.back.contains(&permission),
        }
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
#[serde(default)]
pub struct NetworkSettings {
    pub http: Option<HttpSettings>,
    pub gossipsub: Option<GossipsubSettings>,
}

impl NetworkSettings {
    pub const fn new() -> Self {
        Self {
            http: None,
            gossipsub: None,
        }
    }

    pub fn http(&self) -> &HttpSettings {
        static DEFAULT: HttpSettings = HttpSettings::new();

        self.http.as_ref().unwrap_or(&DEFAULT)
    }

    pub fn into_http(self) -> HttpSettings {
        self.http.unwrap_or_default()
    }

    pub fn gossipsub(&self) -> &GossipsubSettings {
        static DEFAULT: GossipsubSettings = GossipsubSettings::new();

        self.gossipsub.as_ref().unwrap_or(&DEFAULT)
    }

    pub fn into_gossipsub(self) -> GossipsubSettings {
        self.gossipsub.unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct HttpSettings {
    pub methods: HttpMethods,
    pub hosts: HttpHosts,
    #[serde(default = "http_timeout_ms")]
    pub timeout_ms: u64,
}

const fn http_timeout_ms() -> u64 {
    1000 * 10
}

impl HttpSettings {
    pub const fn new() -> Self {
        Self {
            methods: HttpMethods::new(),
            hosts: HttpHosts::new(),
            timeout_ms: http_timeout_ms(),
        }
    }
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone)]
pub enum HttpMethods {
    All,
    List(Vec<HttpMethod>),
}

impl HttpMethods {
    pub const fn new() -> Self {
        Self::All
    }
}

impl Default for HttpMethods {
    fn default() -> Self {
        Self::new()
    }
}

impl Serialize for HttpMethods {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::All => serializer.serialize_str("all"),
            Self::List(methods) => methods.serialize(serializer),
        }
    }
}

struct HttpMethodsVisitor;

impl<'de> de::Visitor<'de> for HttpMethodsVisitor {
    type Value = HttpMethods;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an array of HttpMethod or string \"all\".")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match value.to_lowercase().as_str() {
            "all" => Ok(HttpMethods::All),
            value => Err(E::custom(format!("Unknown string value: {value}"))),
        }
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut methods = Vec::new();
        while let Some(method) = sequence.next_element()? {
            methods.push(method);
        }
        Ok(HttpMethods::List(methods))
    }
}

impl<'de> Deserialize<'de> for HttpMethods {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(HttpMethodsVisitor)
    }
}

#[derive(Debug, Clone)]
pub enum HttpHosts {
    All,
    List(Vec<String>),
}

impl HttpHosts {
    pub const fn new() -> Self {
        Self::All
    }
}

impl Default for HttpHosts {
    fn default() -> Self {
        Self::new()
    }
}

impl Serialize for HttpHosts {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::All => serializer.serialize_str("all"),
            Self::List(hosts) => hosts.serialize(serializer),
        }
    }
}

struct HttpHostsVisitor;

impl<'de> de::Visitor<'de> for HttpHostsVisitor {
    type Value = HttpHosts;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an array of HttpHost or string \"all\".")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match value.to_lowercase().as_str() {
            "all" => Ok(HttpHosts::All),
            value => Err(E::custom(format!("Unknown string value: {value}"))),
        }
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut methods = Vec::new();
        while let Some(method) = sequence.next_element()? {
            methods.push(method);
        }
        Ok(HttpHosts::List(methods))
    }
}

impl<'de> Deserialize<'de> for HttpHosts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(HttpHostsVisitor)
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
#[serde(default)]
pub struct LappManifestSettings {
    #[serde(default)]
    pub lapp_name: String,
    pub application: ApplicationSettings,
    pub required_permissions: PermissionsSettings,
    pub database: Option<DatabaseSettings>,
    pub network: Option<NetworkSettings>,
    pub lapp_requests: Option<Vec<LappRequestsSettings>>,
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

    pub fn network(&self) -> &NetworkSettings {
        static DEFAULT: NetworkSettings = NetworkSettings::new();

        self.network.as_ref().unwrap_or(&DEFAULT)
    }

    pub fn into_network(self) -> NetworkSettings {
        self.network.unwrap_or_default()
    }

    pub fn lapp_requests(&self) -> &[LappRequestsSettings] {
        static DEFAULT: Vec<LappRequestsSettings> = Vec::new();

        self.lapp_requests.as_deref().unwrap_or(&DEFAULT)
    }

    pub fn into_lapp_requests(self) -> Vec<LappRequestsSettings> {
        self.lapp_requests.unwrap_or_default()
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LappUserSettings {
    pub application: ApplicationUserSettings,
    pub allowed_permissions: PermissionsSettings,
    pub lapp_requests: Option<Vec<LappRequestsSettings>>,
}

impl LappUserSettings {
    /// The decisions about a lapp the user has not decided anything about yet: it is enabled and,
    /// if it requires to, may call its own backend, but has no other permission.
    pub fn new(manifest: &LappManifestSettings) -> Self {
        let mut settings = Self::default();
        settings.application.app.title = manifest.application.title.clone();
        settings.application.app.description = manifest.application.description.clone();
        if manifest
            .required_permissions
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
        let required = &manifest.required_permissions;
        PermissionsSettings {
            front: self
                .allowed_permissions
                .front_iter()
                .filter(|permission| permission.is_implicit() || required.front.contains(permission))
                .collect(),
            back: self
                .allowed_permissions
                .back_iter()
                .filter(|permission| permission.is_implicit() || required.back.contains(permission))
                .collect(),
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
            && !((permission.is_implicit() || manifest.required_permissions.contains(permission))
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
        self.allowed_permissions.contains(permission)
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

    pub fn lapp_requests(&self) -> &[LappRequestsSettings] {
        static DEFAULT: Vec<LappRequestsSettings> = Vec::new();

        self.lapp_requests.as_deref().unwrap_or(&DEFAULT)
    }

    pub fn into_lapp_requests(self) -> Vec<LappRequestsSettings> {
        self.lapp_requests.unwrap_or_default()
    }

    pub fn is_front_allowed(&self, permission: FrontPermission) -> bool {
        self.allowed_permissions.front.contains(&permission)
    }

    pub fn is_back_allowed(&self, permission: BackPermission) -> bool {
        self.allowed_permissions.back.contains(&permission)
    }

    pub fn allow_front(&mut self, permission: FrontPermission) -> bool {
        if !self.is_front_allowed(permission) {
            self.allowed_permissions.front.push(permission);
            true
        } else {
            false
        }
    }

    pub fn allow_back(&mut self, permission: BackPermission) -> bool {
        if !self.is_back_allowed(permission) {
            self.allowed_permissions.back.push(permission);
            true
        } else {
            false
        }
    }

    pub fn deny_front(&mut self, permission: FrontPermission) -> bool {
        let index = self
            .allowed_permissions
            .front
            .iter()
            .position(|allowed| *allowed == permission);
        if let Some(index) = index {
            self.allowed_permissions.front.remove(index);
            true
        } else {
            false
        }
    }

    pub fn deny_back(&mut self, permission: BackPermission) -> bool {
        let index = self
            .allowed_permissions
            .back
            .iter()
            .position(|allowed| *allowed == permission);
        if let Some(index) = index {
            self.allowed_permissions.back.remove(index);
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
            required_permissions: PermissionsSettings {
                front: front.to_vec(),
                back: back.to_vec(),
            },
            ..Default::default()
        }
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
        assert_eq!(user.allowed_permissions, PermissionsSettings {
            front: vec![FrontPermission::InvokeBack],
            back: vec![],
        });

        let user = LappUserSettings::new(&lapp_settings(&[], &[]));
        assert_eq!(user.allowed_permissions, PermissionsSettings::default());
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
