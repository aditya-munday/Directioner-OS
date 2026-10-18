use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountState {
    SignedOut,
    SigningIn,
    SignedIn,
    Expiring,
    Expired,
    Revoked,
    OfflineGrace,
}

impl AccountState {
    pub fn can_transition_to(&self, next: AccountState) -> bool {
        match (self, next) {
            (AccountState::SignedOut, AccountState::SigningIn) => true,
            (AccountState::SignedOut, AccountState::SignedIn) => true,
            (AccountState::SigningIn, AccountState::SignedIn) => true,
            (AccountState::SigningIn, AccountState::SignedOut) => true,
            (AccountState::SignedIn, AccountState::Expiring) => true,
            (AccountState::SignedIn, AccountState::OfflineGrace) => true,
            (AccountState::SignedIn, AccountState::Revoked) => true,
            (AccountState::SignedIn, AccountState::SignedOut) => true,
            (AccountState::Expiring, AccountState::SignedIn) => true,
            (AccountState::Expiring, AccountState::Expired) => true,
            (AccountState::Expiring, AccountState::Revoked) => true,
            (AccountState::Expiring, AccountState::SignedOut) => true,
            (AccountState::Expired, AccountState::SignedIn) => true,
            (AccountState::Expired, AccountState::SignedOut) => true,
            (AccountState::OfflineGrace, AccountState::SignedIn) => true,
            (AccountState::OfflineGrace, AccountState::Expired) => true,
            (AccountState::OfflineGrace, AccountState::Revoked) => true,
            (AccountState::OfflineGrace, AccountState::SignedOut) => true,
            (AccountState::Revoked, AccountState::SignedOut) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Audience {
    Os,
    Cli,
    Aria,
    Cloud,
    Update,
}

impl Audience {
    pub fn as_str(&self) -> &'static str {
        match self {
            Audience::Os => "os",
            Audience::Cli => "cli",
            Audience::Aria => "aria",
            Audience::Cloud => "cloud",
            Audience::Update => "update",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "os" => Some(Audience::Os),
            "cli" => Some(Audience::Cli),
            "aria" => Some(Audience::Aria),
            "cloud" => Some(Audience::Cloud),
            "update" => Some(Audience::Update),
            _ => None,
        }
    }

    pub fn allowed_scopes(&self) -> &'static [&'static str] {
        match self {
            Audience::Os => &["os:read", "os:status"],
            Audience::Cli => &["cli:exec", "cli:status"],
            Audience::Aria => &["aria:voice", "aria:assist"],
            Audience::Cloud => &["cloud:storage", "cloud:sync"],
            Audience::Update => &["update:check", "update:apply"],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub created_at: i64,
    pub audience: String,
    pub scopes: Vec<String>,
}

impl Drop for AuthToken {
    fn drop(&mut self) {
        self.access_token.clear();
        self.refresh_token.clear();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRegistration {
    pub device_id: String,
    pub device_name: String,
    pub os_version: String,
    pub variant_id: String,
    pub registered_at: i64,
    pub is_current: bool,
    pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entitlements {
    pub plan: String,
    pub features: Vec<String>,
    pub quota_remaining: u64,
    pub reset_epoch: i64,
    pub grace_expiry: i64,
    pub signature: String,
    pub last_verified_clock: i64,
}

impl Entitlements {
    pub fn is_valid_offline(&self, now: i64) -> bool {
        if now < self.last_verified_clock {
            return false;
        }
        now <= self.grace_expiry
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountEvent {
    LoggedIn { user_hash: String, device_id: String },
    LoggedOut { device_id: String },
    TokenRefreshed { audience: String },
    QuotaWarning { remaining: u64 },
    Revoked { reason: String },
    OfflineGraceActive { expires_at: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub user_hash: String,
    pub email_hash: String,
    pub org_id: Option<String>,
}
