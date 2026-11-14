use anyhow::Result;
use base64::Engine as _;
use rand::RngCore;
use sha2::{Digest, Sha256};

pub fn generate_pkce() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

    let hash = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash);

    (code_verifier, code_challenge)
}

pub fn generate_state() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

const ISSUER: &str = match option_env!("NORDENVAULT_KEYCLOAK_ISSUER") {
    Some(issuer) => issuer,
    None => "https://auth.nordenvault.com/realms/nordenvault",
};
const CLIENT_ID: &str = "nordenvault-desktop";

pub fn build_auth_url(
    code_challenge: &str,
    state: &str,
    redirect_uri: &str,
) -> String {
    format!(
        "{ISSUER}/protocol/openid-connect/auth\
        ?client_id={}\
        &redirect_uri={}\
        &response_type=code\
        &code_challenge={}\
        &code_challenge_method=S256\
        &state={}\
        &scope=openid%20email%20profile",
        CLIENT_ID,
        urlencoding::encode(redirect_uri),
        code_challenge,
        state,
    )
}

#[derive(Debug, serde::Deserialize)]
pub struct IdentityTokenResponse {
    pub access_token: String,
}

pub async fn exchange_code(
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<IdentityTokenResponse> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{ISSUER}/protocol/openid-connect/token"))
        .form(&serde_json::json!({
            "client_id": CLIENT_ID,
            "code": code,
            "code_verifier": code_verifier,
            "grant_type": "authorization_code",
            "redirect_uri": redirect_uri,
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<IdentityTokenResponse>()
        .await?;
    Ok(resp)
}
