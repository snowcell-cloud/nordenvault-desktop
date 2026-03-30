use anyhow::{anyhow, Result};

// In release builds use the macOS Keychain (secure, signed app won't prompt).
// In debug builds use a plain JSON file so the unsigned binary never triggers
// the repeated "nordenvault-desktop wants to use confidential information" dialog.
#[cfg(not(debug_assertions))]
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

const SERVICE: &str = "nordenvault-desktop";
const ACCESS_TOKEN_ACCOUNT: &str = "access_token";
const REFRESH_TOKEN_ACCOUNT: &str = "refresh_token";
const S3_SECRET_ACCOUNT: &str = "s3_secret_key";

// ── debug helpers ──────────────────────────────────────────────────────────

#[cfg(debug_assertions)]
fn dev_secrets_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("nordenvault")
        .join("dev_secrets.json")
}

#[cfg(debug_assertions)]
fn dev_read(key: &str) -> Option<String> {
    let path = dev_secrets_path();
    let data = std::fs::read_to_string(&path).ok()?;
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&data).ok()?;
    map.get(key)?.as_str().map(String::from)
}

#[cfg(debug_assertions)]
fn dev_write(key: &str, value: &str) -> Result<()> {
    let path = dev_secrets_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut map: serde_json::Map<String, serde_json::Value> = path
        .exists()
        .then(|| {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
        })
        .flatten()
        .unwrap_or_default();
    map.insert(key.to_string(), serde_json::Value::String(value.to_string()));
    std::fs::write(&path, serde_json::to_string_pretty(&map)?)?;
    Ok(())
}

#[cfg(debug_assertions)]
fn dev_delete(key: &str) {
    let path = dev_secrets_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(mut map) =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&data)
        {
            map.remove(key);
            let _ = std::fs::write(&path, serde_json::to_string_pretty(&map).unwrap_or_default());
        }
    }
}

// ── public API ─────────────────────────────────────────────────────────────

pub fn store_tokens(access: &str, refresh: &str) -> Result<()> {
    #[cfg(debug_assertions)]
    {
        dev_write(ACCESS_TOKEN_ACCOUNT, access)?;
        dev_write(REFRESH_TOKEN_ACCOUNT, refresh)?;
        return Ok(());
    }
    #[cfg(not(debug_assertions))]
    {
        set_generic_password(SERVICE, ACCESS_TOKEN_ACCOUNT, access.as_bytes())
            .map_err(|e| anyhow!("Failed to store access token: {}", e))?;
        set_generic_password(SERVICE, REFRESH_TOKEN_ACCOUNT, refresh.as_bytes())
            .map_err(|e| anyhow!("Failed to store refresh token: {}", e))?;
        Ok(())
    }
}

pub fn load_tokens() -> Option<(String, String)> {
    #[cfg(debug_assertions)]
    {
        let access = dev_read(ACCESS_TOKEN_ACCOUNT)?;
        let refresh = dev_read(REFRESH_TOKEN_ACCOUNT)?;
        return Some((access, refresh));
    }
    #[cfg(not(debug_assertions))]
    {
        let access = get_generic_password(SERVICE, ACCESS_TOKEN_ACCOUNT).ok()?;
        let refresh = get_generic_password(SERVICE, REFRESH_TOKEN_ACCOUNT).ok()?;
        Some((
            String::from_utf8(access).ok()?,
            String::from_utf8(refresh).ok()?,
        ))
    }
}

pub fn delete_tokens() {
    #[cfg(debug_assertions)]
    {
        dev_delete(ACCESS_TOKEN_ACCOUNT);
        dev_delete(REFRESH_TOKEN_ACCOUNT);
        return;
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = delete_generic_password(SERVICE, ACCESS_TOKEN_ACCOUNT);
        let _ = delete_generic_password(SERVICE, REFRESH_TOKEN_ACCOUNT);
    }
}

pub fn store_s3_secret(secret: &str) -> Result<()> {
    #[cfg(debug_assertions)]
    {
        return dev_write(S3_SECRET_ACCOUNT, secret);
    }
    #[cfg(not(debug_assertions))]
    {
        set_generic_password(SERVICE, S3_SECRET_ACCOUNT, secret.as_bytes())
            .map_err(|e| anyhow!("Failed to store S3 secret: {}", e))
    }
}

pub fn load_s3_secret() -> Option<String> {
    #[cfg(debug_assertions)]
    {
        return dev_read(S3_SECRET_ACCOUNT);
    }
    #[cfg(not(debug_assertions))]
    {
        let bytes = get_generic_password(SERVICE, S3_SECRET_ACCOUNT).ok()?;
        String::from_utf8(bytes).ok()
    }
}

pub fn delete_s3_secret() {
    #[cfg(debug_assertions)]
    {
        dev_delete(S3_SECRET_ACCOUNT);
        return;
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = delete_generic_password(SERVICE, S3_SECRET_ACCOUNT);
    }
}
