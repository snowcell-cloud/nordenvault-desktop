pub mod callback_server;
pub mod keychain;
pub mod oauth;
pub mod token;

use uuid::Uuid;

#[derive(Debug, Default)]
pub struct AuthState {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub user_id: Option<Uuid>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub pending_pkce: Option<PkceState>,
}

impl AuthState {
    pub fn to_dto(&self) -> AuthStateDto {
        AuthStateDto {
            is_logged_in: self.access_token.is_some(),
            email: self.email.clone(),
            name: self.name.clone(),
            user_id: self.user_id.map(|id| id.to_string()),
        }
    }
}

#[derive(Debug)]
pub struct PkceState {
    pub code_verifier: String,
    pub state: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStateDto {
    pub is_logged_in: bool,
    pub email: Option<String>,
    pub name: Option<String>,
    pub user_id: Option<String>,
}
