use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Claims {
    exp: u64,
}

pub fn is_expired(token: &str) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    match decode_exp(token) {
        Some(exp) => now >= exp,
        None => true,
    }
}

pub fn should_refresh(token: &str) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    match decode_exp(token) {
        Some(exp) => now + 90 >= exp,
        None => true,
    }
}

fn decode_exp(token: &str) -> Option<u64> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;
    let claims: Claims = serde_json::from_slice(&payload).ok()?;
    Some(claims.exp)
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
}

pub async fn refresh(refresh_token: &str, api_base: &str) -> Result<TokenResponse> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/auth/refresh", api_base))
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await?
        .error_for_status()?
        .json::<TokenResponse>()
        .await?;
    Ok(resp)
}

use base64::Engine as _;
