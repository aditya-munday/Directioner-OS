use crate::types::{AccountState, AuthToken, DeviceRegistration, Entitlements, UserProfile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "params")]
pub enum AccountRequest {
    LoginStartDevice,
    LoginPollDevice { device_code: String },
    LoginPkce { verifier: String, challenge: String },
    RefreshToken,
    Logout,
    Status,
    Whoami,
    TokenGet { audience: String },
    DeviceList,
    DeviceRevoke { device_id: String },
    EntitlementsGet,
    ExportData,
    DeleteAccount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "data")]
pub enum AccountResponse {
    Ok(ResponseData),
    Error { code: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseData {
    DeviceCodeInitiated {
        device_code: String,
        user_code: String,
        verification_uri: String,
        interval: u64,
    },
    TokenIssued(AuthToken),
    StatusInfo {
        state: AccountState,
        user: Option<UserProfile>,
        device_id: String,
        plan: Option<String>,
        offline_grace_remaining_secs: Option<i64>,
    },
    WhoamiInfo {
        user_id: String,
        user_hash: String,
        device_name: String,
    },
    DeviceCollection(Vec<DeviceRegistration>),
    EntitlementsInfo(Entitlements),
    LoggedOut,
    DeviceRevoked {
        device_id: String,
    },
    ExportArchive {
        json_payload: String,
    },
    AccountDeleted,
}
