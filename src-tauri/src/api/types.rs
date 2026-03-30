use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct NordenVaultTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DesktopStatusResponse {
    pub has_organization: bool,
    pub has_storage: bool,
    pub machine_id: Option<Uuid>,
    pub machine_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DesktopProvisionRequest {
    pub hostname: String,
    pub platform: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DesktopProvisionResponse {
    pub org_id: Uuid,
    pub machine_id: Uuid,
    pub machine_name: String,
    pub gateway_url: String,
    pub bucket_name: String,
    pub endpoint_url: String,
    pub region: String,
    pub credential_id: Uuid,
    pub access_key_id: String,
    pub secret_access_key: String,
}
