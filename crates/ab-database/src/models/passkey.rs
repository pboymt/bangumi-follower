use sqlx::FromRow;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Passkey {
    pub id: i32,
    pub user_id: i32,
    pub name: String,
    pub credential_id: String,
    pub public_key: String,
    pub sign_count: i32,
    pub aaguid: Option<String>,
    pub transports: Option<String>,
    pub created_at: Option<String>,
    pub last_used_at: Option<String>,
    pub backup_eligible: bool,
    pub backup_state: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyCreate {
    pub credential_id: String,
    pub public_key: String,
    pub name: String,
    pub user_id: i32,
    pub aaguid: Option<String>,
    pub transports: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyList {
    pub id: i32,
    pub name: String,
    pub credential_id: String,
    pub created_at: Option<String>,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyDelete {
    pub passkey_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyAuthStart {
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyAuthFinish {
    pub credential: serde_json::Value,
}
