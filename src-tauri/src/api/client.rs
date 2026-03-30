use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::auth::{keychain, AuthState};
use crate::auth::token;

pub struct ApiClient {
    pub base_url: String,
    pub http: reqwest::Client,
    pub auth: Arc<Mutex<AuthState>>,
}

impl ApiClient {
    pub fn new(base_url: String, auth: Arc<Mutex<AuthState>>) -> Self {
        Self {
            base_url,
            http: reqwest::Client::builder()
                .user_agent("NordenVault-Desktop/0.1")
                .build()
                .unwrap(),
            auth,
        }
    }

    pub async fn access_token(&self) -> Result<String> {
        let mut auth = self.auth.lock().await;

        let needs_refresh = match &auth.access_token {
            Some(t) => token::should_refresh(t),
            None => return Err(anyhow!("Not authenticated")),
        };

        if needs_refresh {
            let refresh_token = auth.refresh_token.as_deref()
                .ok_or_else(|| anyhow!("No refresh token"))?;
            let pair = token::refresh(refresh_token, &self.base_url).await?;
            keychain::store_tokens(&pair.access_token, &pair.refresh_token)?;
            auth.access_token = Some(pair.access_token.clone());
            auth.refresh_token = Some(pair.refresh_token);
            return Ok(pair.access_token);
        }

        Ok(auth.access_token.clone().unwrap())
    }

    pub async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let token = self.access_token().await?;
        let resp = self.http
            .get(format!("{}{}", self.base_url, path))
            .bearer_auth(token)
            .send()
            .await?;
        let resp = check_status(resp).await?;
        Ok(resp.json::<T>().await?)
    }

    pub async fn post<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let token = self.access_token().await?;
        let resp = self.http
            .post(format!("{}{}", self.base_url, path))
            .bearer_auth(token)
            .json(body)
            .send()
            .await?;
        let resp = check_status(resp).await?;
        Ok(resp.json::<T>().await?)
    }

}

async fn check_status(resp: reqwest::Response) -> Result<reqwest::Response> {
    if resp.status().is_client_error() || resp.status().is_server_error() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("HTTP {} — {}", status, body));
    }
    Ok(resp)
}

impl ApiClient {
    pub async fn post_workos_token(&self, workos_token: &str) -> Result<crate::api::types::NordenVaultTokenResponse> {
        let resp = self.http
            .post(format!("{}/api/auth/bootstrap", self.base_url))
            .bearer_auth(workos_token)
            .send()
            .await?
            .error_for_status()?
            .json::<crate::api::types::NordenVaultTokenResponse>()
            .await?;
        Ok(resp)
    }
}
