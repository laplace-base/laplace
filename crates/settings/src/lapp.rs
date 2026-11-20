use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
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

    /// In a manifest: the user may allow hosts it does not list.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub user_hosts: bool,
}

impl FrontHttpSettings {
    pub const fn new() -> Self {
        Self {
            hosts: HttpHosts::new(),
            user_hosts: false,
        }
    }

    pub const fn is_empty(&self) -> bool {
        let Self { hosts, user_hosts } = self;
        hosts.is_empty() && !*user_hosts
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackHttpSettings {
    #[serde(default, skip_serializing_if = "HttpHosts::is_empty")]
    pub hosts: HttpHosts,

    /// The methods of the hosts without methods of their own.
    #[serde(default, skip_serializing_if = "HttpMethods::is_empty")]
    pub methods: HttpMethods,

    /// In a manifest: the user may allow hosts it does not list.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub user_hosts: bool,

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
            user_hosts: false,
            timeout_ms: Self::default_timeout_ms(),
        }
    }

    pub const fn is_empty(&self) -> bool {
        let Self {
            hosts,
            methods,
            user_hosts,
            timeout_ms,
        } = self;
        hosts.is_empty() && methods.is_empty() && !*user_hosts && Self::is_timeout_default(timeout_ms)
    }

    const fn is_timeout_default(timeout_ms: &u64) -> bool {
        *timeout_ms == Self::default_timeout_ms()
    }

    const fn default_timeout_ms() -> u64 {
        1000 * 10
    }
}

/// The hosts either half of the lapp actually reaches.
#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct HttpHostsSettings {
    pub front: EffectiveHosts,
    pub back: EffectiveHosts,
}

/// The hosts a half of the lapp actually reaches, every one with its methods: those the manifest
/// asks for and the user allows, narrowed by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectiveHosts {
    All(HttpMethods),
    List(Vec<EffectiveHost>),
}

impl Default for EffectiveHosts {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

impl EffectiveHosts {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::List(hosts) if hosts.is_empty())
    }

    /// The hosts of the manifest that the user allows.
    ///
    /// The methods of a host are its own of the manifest, or `default`, narrowed by those of the
    /// user. A host the manifest does not list is allowed only with `user_hosts`, which `"all"` of
    /// the manifest implies; `"all"` of the user is every host of the manifest.
    pub fn resolve(requested: &HttpHosts, user_hosts: bool, default: &HttpMethods, allowed: &HttpHosts) -> Self {
        let narrowed = |methods: &HttpMethods, entry: &HttpHostEntry| match &entry.methods {
            Some(narrowing) => methods.intersect(narrowing),
            None => methods.clone(),
        };

        match (requested, allowed) {
            (HttpHosts::All, HttpHosts::All) => Self::All(default.clone()),
            (HttpHosts::List(requested), HttpHosts::All) => Self::List(
                requested
                    .iter()
                    .map(|entry| EffectiveHost {
                        host: entry.host.clone(),
                        methods: entry.methods.clone().unwrap_or_else(|| default.clone()),
                    })
                    .collect(),
            ),
            (requested, HttpHosts::List(allowed)) => Self::List(
                allowed
                    .iter()
                    .filter_map(|entry| {
                        let methods = match requested {
                            HttpHosts::All => default,
                            HttpHosts::List(requested) => match requested.iter().find(|asked| asked.is(&entry.host)) {
                                Some(asked) => asked.methods.as_ref().unwrap_or(default),
                                None if user_hosts => default,
                                None => return None,
                            },
                        };
                        Some(EffectiveHost {
                            host: entry.host.clone(),
                            methods: narrowed(methods, entry),
                        })
                    })
                    .collect(),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Patch,
    Options,
}

impl HttpMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Options => "OPTIONS",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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

    /// The methods among both.
    pub fn intersect(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::All, other) => other.clone(),
            (this, Self::All) => this.clone(),
            (Self::List(these), Self::List(those)) => {
                Self::List(these.iter().copied().filter(|method| those.contains(method)).collect())
            },
        }
    }

    /// Whether the HTTP method `method` (`GET`, `post`, …) is among these.
    pub fn allows(&self, method: &str) -> bool {
        match self {
            Self::All => true,
            Self::List(methods) => methods
                .iter()
                .any(|allowed| allowed.as_str().eq_ignore_ascii_case(method)),
        }
    }
}

/// Hosts of external HTTP access: `"all"` or a list; empty by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HttpHosts {
    #[serde(with = "all_str")]
    All,

    List(Vec<HttpHostEntry>),
}

/// A host entry — `host`, `host:port` or `*.domain` — with its methods: as they are written
/// ([`HttpHostEntry`]) or as they come into effect ([`EffectiveHost`]).
///
/// In TOML an entry without methods of its own is a string, `"api.example.org"`, and one with them a
/// table, `{ host = "api.example.org", methods = ["get"] }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry<M> {
    pub host: String,
    pub methods: M,
}

/// An entry of a manifest or of the user decisions; `None` is no methods of its own: the default ones
/// of the manifest, or no narrowing by the user.
pub type HttpHostEntry = HostEntry<Option<HttpMethods>>;

/// An effective entry: its methods are those that come into effect.
pub type EffectiveHost = HostEntry<HttpMethods>;

impl<M> HostEntry<M> {
    /// Whether this is the entry of `host`, as it is written.
    pub fn is(&self, host: &str) -> bool {
        self.host.eq_ignore_ascii_case(host)
    }
}

impl From<&str> for HttpHostEntry {
    fn from(host: &str) -> Self {
        host.to_owned().into()
    }
}

impl From<String> for HttpHostEntry {
    fn from(host: String) -> Self {
        Self { host, methods: None }
    }
}

/// The methods of a [`HostEntry`]; unset ones are written as a bare host string.
pub trait EntryMethods: Default {
    fn is_unset(&self) -> bool;
}

impl EntryMethods for Option<HttpMethods> {
    fn is_unset(&self) -> bool {
        self.is_none()
    }
}

impl EntryMethods for HttpMethods {
    fn is_unset(&self) -> bool {
        false
    }
}

#[derive(Deserialize)]
#[serde(untagged, bound = "M: Deserialize<'de> + Default")]
enum HostEntryRepr<M> {
    Host(String),
    Table {
        host: String,
        #[serde(default)]
        methods: M,
    },
}

impl<'de, M: EntryMethods + Deserialize<'de>> Deserialize<'de> for HostEntry<M> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match HostEntryRepr::<M>::deserialize(deserializer)? {
            HostEntryRepr::Host(host) => Self {
                host,
                methods: M::default(),
            },
            HostEntryRepr::Table { host, methods } => Self { host, methods },
        })
    }
}

impl<M: EntryMethods + Serialize> Serialize for HostEntry<M> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Table<'a, M> {
            host: &'a str,
            methods: &'a M,
        }

        if self.methods.is_unset() {
            serializer.serialize_str(&self.host)
        } else {
            Table {
                host: &self.host,
                methods: &self.methods,
            }
            .serialize(serializer)
        }
    }
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

    fn has_methods(&self) -> bool {
        matches!(self, Self::List(hosts) if hosts.iter().any(|entry| entry.methods.is_some()))
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

impl LappSettings {
    /// What the format allows but the platform cannot do.
    pub fn validate(&self) -> Result<(), String> {
        if self.network.http.front.hosts.has_methods() {
            return Err("the front hosts cannot have methods: the CSP of the lapp window does not limit them".into());
        }
        Ok(())
    }
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

        HttpHostsSettings {
            front: if permissions.front.contains(&FrontPermission::Http) {
                EffectiveHosts::resolve(
                    &requested.front.hosts,
                    requested.front.user_hosts,
                    &HttpMethods::All,
                    &allowed.front.hosts,
                )
            } else {
                EffectiveHosts::default()
            },
            back: if permissions.back.contains(&BackPermission::Http) {
                EffectiveHosts::resolve(
                    &requested.back.hosts,
                    requested.back.user_hosts,
                    &requested.back.methods,
                    &allowed.back.hosts,
                )
            } else {
                EffectiveHosts::default()
            },
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
        HttpHosts::List(hosts.iter().map(|&host| host.into()).collect())
    }

    #[cfg(feature = "io")]
    #[test]
    fn http_methods_parse_and_allow() {
        #[derive(Deserialize)]
        struct Methods {
            methods: HttpMethods,
        }

        let listed: Methods =
            toml::from_str(r#"methods = ["get", "put", "delete", "patch", "head", "options"]"#).unwrap();
        assert!(listed.methods.allows("GET"));
        assert!(listed.methods.allows("patch"));
        assert!(!listed.methods.allows("POST"));

        let all: Methods = toml::from_str(r#"methods = "all""#).unwrap();
        assert!(all.methods.allows("TRACE"));
        assert!(!HttpMethods::List(Vec::new()).allows("GET"));
    }

    fn effective(entries: &[(&str, HttpMethods)]) -> EffectiveHosts {
        EffectiveHosts::List(
            entries
                .iter()
                .map(|(host, methods)| EffectiveHost {
                    host: (*host).into(),
                    methods: methods.clone(),
                })
                .collect(),
        )
    }

    fn methods(methods: &[HttpMethod]) -> HttpMethods {
        HttpMethods::List(methods.to_vec())
    }

    fn entry(host: &str, entry_methods: &[HttpMethod]) -> HttpHostEntry {
        HttpHostEntry {
            host: host.into(),
            methods: Some(methods(entry_methods)),
        }
    }

    #[cfg(feature = "io")]
    #[test]
    fn host_entries_parse_as_strings_tables_and_all() {
        #[derive(Deserialize, Serialize)]
        struct Hosts {
            hosts: HttpHosts,
        }

        let mixed: Hosts =
            toml::from_str(r#"hosts = ["api.org", { host = "up.org", methods = ["post"] }, { host = "x.org" }]"#)
                .unwrap();
        assert_eq!(
            mixed.hosts,
            HttpHosts::List(vec![
                "api.org".into(),
                entry("up.org", &[HttpMethod::Post]),
                "x.org".into()
            ])
        );
        assert_eq!(
            toml::to_string(&mixed).unwrap(),
            "hosts = [\"api.org\", { host = \"up.org\", methods = [\"post\"] }, \"x.org\"]\n"
        );

        let all: Hosts = toml::from_str(r#"hosts = "all""#).unwrap();
        assert_eq!(all.hosts, HttpHosts::All);
    }

    #[cfg(feature = "io")]
    #[test]
    fn effective_host_is_always_a_table() {
        #[derive(Deserialize, Serialize)]
        struct Hosts {
            hosts: Vec<EffectiveHost>,
        }

        let hosts = Hosts {
            hosts: vec![EffectiveHost {
                host: "api.org".into(),
                methods: HttpMethods::All,
            }],
        };
        let written = toml::to_string(&hosts).unwrap();
        assert_eq!(written, "[[hosts]]\nhost = \"api.org\"\nmethods = \"all\"\n");
        assert_eq!(toml::from_str::<Hosts>(&written).unwrap().hosts, hosts.hosts);
    }

    #[test]
    fn front_hosts_have_no_methods() {
        let mut settings = LappSettings::default();
        settings.network.http.front.hosts = HttpHosts::List(vec!["api.org".into()]);
        assert!(settings.validate().is_ok());

        settings.network.http.front.hosts = HttpHosts::List(vec![entry("api.org", &[HttpMethod::Get])]);
        assert!(settings.validate().unwrap_err().contains("CSP"));
    }

    #[test]
    fn effective_hosts_are_the_allowed_of_the_requested_with_narrowed_methods() {
        use HttpMethod::*;

        let requested = HttpHosts::List(vec!["a.org".into(), entry("b.org", &[Get, Post])]);
        let default = methods(&[Get]);
        let resolve =
            |user_hosts, allowed: &HttpHosts| EffectiveHosts::resolve(&requested, user_hosts, &default, allowed);

        // "all" of the user is every host of the manifest, not any host.
        let all = effective(&[("a.org", methods(&[Get])), ("b.org", methods(&[Get, Post]))]);
        assert_eq!(resolve(false, &HttpHosts::All), all);
        assert_eq!(resolve(true, &HttpHosts::All), all);

        let allowed = HttpHosts::List(vec!["B.org".into(), entry("a.org", &[Post]), "c.org".into()]);
        assert_eq!(
            resolve(false, &allowed),
            effective(&[("B.org", methods(&[Get, Post])), ("a.org", methods(&[]))])
        );
        assert_eq!(
            resolve(true, &allowed),
            effective(&[
                ("B.org", methods(&[Get, Post])),
                ("a.org", methods(&[])),
                ("c.org", methods(&[Get]))
            ])
        );

        let narrowed = HttpHosts::List(vec![entry("b.org", &[Post, Delete])]);
        assert_eq!(resolve(false, &narrowed), effective(&[("b.org", methods(&[Post]))]));
        assert!(resolve(false, &HttpHosts::new()).is_empty());

        // "all" of the manifest lets the user name any host, and "all" itself.
        let any = |allowed: &HttpHosts| EffectiveHosts::resolve(&HttpHosts::All, false, &default, allowed);
        assert_eq!(any(&HttpHosts::All), EffectiveHosts::All(methods(&[Get])));
        assert_eq!(
            any(&HttpHosts::List(vec!["z.org".into()])),
            effective(&[("z.org", methods(&[Get]))])
        );
    }

    #[test]
    fn effective_http_hosts_need_the_permission_and_the_user_choice() {
        let mut manifest = lapp_settings(&[FrontPermission::Http], &[BackPermission::Http]);
        manifest.required.network = NetworkSettings {
            http: HttpSettings {
                front: FrontHttpSettings {
                    hosts: hosts(&["cdn.org", "api.org"]),
                    ..FrontHttpSettings::new()
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
                ..FrontHttpSettings::new()
            },
            back: BackHttpSettings {
                hosts: hosts(&["any.org"]),
                ..BackHttpSettings::new()
            },
        };
        assert_eq!(user.effective_http_hosts(&manifest), HttpHostsSettings {
            front: effective(&[("api.org", HttpMethods::All)]),
            back: EffectiveHosts::default(),
        });

        user.allow_back(BackPermission::Http);
        assert_eq!(
            user.effective_http_hosts(&manifest).back,
            effective(&[("any.org", HttpMethods::All)])
        );

        // The front hosts the user adds go only with `user_hosts` of the manifest.
        manifest.required.network.http.front.user_hosts = true;
        assert_eq!(
            user.effective_http_hosts(&manifest).front,
            effective(&[("api.org", HttpMethods::All), ("other.org", HttpMethods::All)])
        );
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
