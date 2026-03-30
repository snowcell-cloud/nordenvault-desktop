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

pub fn build_auth_url(
    workos_client_id: &str,
    code_challenge: &str,
    state: &str,
    redirect_uri: &str,
) -> String {
    format!(
        "https://api.workos.com/user_management/authorize\
        ?client_id={}\
        &redirect_uri={}\
        &response_type=code\
        &code_challenge={}\
        &code_challenge_method=S256\
        &state={}\
        &provider=authkit",
        workos_client_id,
        urlencoding::encode(redirect_uri),
        code_challenge,
        state,
    )
}

#[derive(Debug, serde::Deserialize)]
pub struct WorkosTokenResponse {
    pub access_token: String,
}

pub async fn exchange_code(
    client_id: &str,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<WorkosTokenResponse> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.workos.com/user_management/authenticate")
        .json(&serde_json::json!({
            "client_id": client_id,
            "code": code,
            "code_verifier": code_verifier,
            "grant_type": "authorization_code",
            "redirect_uri": redirect_uri,
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<WorkosTokenResponse>()
        .await?;
    Ok(resp)
}
