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
}
