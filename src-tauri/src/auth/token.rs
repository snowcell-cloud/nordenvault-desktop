use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Claims {
    exp: u64,
}

#[allow(dead_code)]
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

/// Revoke the application session before deleting this device's saved tokens.
pub async fn revoke(token: &str, api_base: &str) -> Result<()> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?
        .post(format!("{}/api/auth/logout", api_base))
        .json(&serde_json::json!({ "refresh_token": token }))
        .send().await?.error_for_status()?;
    Ok(())
}

#[cfg(test)]
mod session_tests {
    use std::io::{Read, Write};
    fn endpoint(status: &str) -> (String, std::thread::JoinHandle<()>) {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.local_addr().unwrap());
        let status = status.to_string();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = server.accept().unwrap();
            stream.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
            let mut data = Vec::new();
            let mut buf = [0u8; 1024];
            while !String::from_utf8_lossy(&data).contains("synthetic-session-token") {
                let size = stream.read(&mut buf).unwrap();
                assert!(size > 0); data.extend_from_slice(&buf[..size]);
            }
            let request = String::from_utf8_lossy(&data);
            assert!(request.starts_with("POST /api/auth/logout HTTP/1.1"));
            assert!(request.contains("\"refresh_token\":\"synthetic-session-token\""));
            write!(stream, "HTTP/1.1 {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", status).unwrap();
        });
        (url, handle)
    }
    #[tokio::test]
    async fn successful_revoke_accepts_empty_204() {
        let (url, server) = endpoint("204 No Content");
        super::revoke("synthetic-session-token", &url).await.unwrap(); server.join().unwrap();
    }
    #[tokio::test]
    async fn failed_revoke_is_reported_to_signout_caller() {
        let (url, server) = endpoint("503 Service Unavailable");
        assert!(super::revoke("synthetic-session-token", &url).await.is_err()); server.join().unwrap();
    }
}
