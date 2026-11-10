use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString, IntoStaticStr};

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, AsRefStr, IntoStaticStr, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum FrontPermission {
    InvokeBack,
    Http,
    Websocket,
    LappsOutgoing,
}

impl FrontPermission {
    pub fn as_str(&self) -> &'static str {
        self.into()
    }

    /// Whether the lapp has the permission as soon as its manifest requires it.
    pub fn is_implicit(&self) -> bool {
        matches!(self, Self::LappsOutgoing)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, AsRefStr, IntoStaticStr, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum BackPermission {
    FileRead,
    FileWrite,
    Http,
    Websocket,
    Tcp,
    Database,
    LappsIncoming,
    LappsOutgoing,
}

impl BackPermission {
    pub fn as_str(&self) -> &'static str {
        self.into()
    }

    /// Whether the lapp has the permission as soon as its manifest requires it.
    pub fn is_implicit(&self) -> bool {
        matches!(self, Self::LappsIncoming | Self::LappsOutgoing)
    }
}

/// A permission of either half of a lapp, serialized as `{ "front": "invoke_back" }`.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Front(FrontPermission),
    Back(BackPermission),
}

impl Permission {
    pub fn is_implicit(&self) -> bool {
        match self {
            Self::Front(permission) => permission.is_implicit(),
            Self::Back(permission) => permission.is_implicit(),
        }
    }
}

impl From<FrontPermission> for Permission {
    fn from(permission: FrontPermission) -> Self {
        Self::Front(permission)
    }
}

impl From<BackPermission> for Permission {
    fn from(permission: BackPermission) -> Self {
        Self::Back(permission)
    }
}
