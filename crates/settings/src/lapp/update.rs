use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::lapp::access::Permission;

/// A change of the user decisions about a lapp; as a response, the part of it that was applied.
#[skip_serializing_none]
#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct UpdateQuery {
    pub lapp_name: String,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub autoload: Option<bool>,
    #[serde(default)]
    pub allow_permission: Option<Permission>,
    #[serde(default)]
    pub deny_permission: Option<Permission>,
}

impl UpdateQuery {
    pub fn new(lapp_name: impl Into<String>) -> Self {
        Self {
            lapp_name: lapp_name.into(),
            ..Default::default()
        }
    }

    pub fn is_applied(&self) -> bool {
        let Self {
            lapp_name: _,
            enabled,
            autoload,
            allow_permission,
            deny_permission,
        } = self;
        enabled.is_some() || autoload.is_some() || allow_permission.is_some() || deny_permission.is_some()
    }

    pub fn enabled(mut self, enabled: impl Into<Option<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn autoload(mut self, autoload: impl Into<Option<bool>>) -> Self {
        self.autoload = autoload.into();
        self
    }

    pub fn allow_permission(mut self, permission: impl Into<Permission>) -> Self {
        self.allow_permission = Some(permission.into());
        self
    }

    pub fn deny_permission(mut self, permission: impl Into<Permission>) -> Self {
        self.deny_permission = Some(permission.into());
        self
    }

    pub fn update_permission(self, permission: impl Into<Permission>, allow: bool) -> Self {
        if allow {
            self.allow_permission(permission)
        } else {
            self.deny_permission(permission)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lapp::access::{BackPermission, FrontPermission};

    #[test]
    fn serialize_query() {
        let json = serde_json::to_string(&UpdateQuery::new("test")).unwrap();
        assert_eq!(json, r#"{"lapp_name":"test"}"#);

        let json = serde_json::to_string(&UpdateQuery::new("test").enabled(true)).unwrap();
        assert_eq!(json, r#"{"lapp_name":"test","enabled":true}"#);

        let query = UpdateQuery::new("test")
            .enabled(true)
            .autoload(true)
            .allow_permission(FrontPermission::InvokeBack)
            .deny_permission(BackPermission::Tcp);
        let json = serde_json::to_string(&query).unwrap();
        assert_eq!(
            json,
            r#"{"lapp_name":"test","enabled":true,"autoload":true,"allow_permission":{"front":"invoke_back"},"deny_permission":{"back":"tcp"}}"#
        );
    }

    #[test]
    fn deserialize_query() {
        let query: UpdateQuery = serde_json::from_str(r#"{"lapp_name":"test"}"#).unwrap();
        assert_eq!(query, UpdateQuery::new("test"));
        assert!(!query.is_applied());

        let query: UpdateQuery =
            serde_json::from_str(r#"{"lapp_name":"test","autoload":false,"allow_permission":{"back":"http"}}"#)
                .unwrap();
        assert_eq!(
            query,
            UpdateQuery::new("test")
                .autoload(false)
                .allow_permission(BackPermission::Http)
        );
        assert!(query.is_applied());
    }
}
