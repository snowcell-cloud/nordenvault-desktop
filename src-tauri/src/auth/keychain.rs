use anyhow::Result;

// Use macOS Keychain only on macOS release builds.
// On Linux, Windows, and all debug builds, use a plain JSON file.
#[cfg(all(target_os = "macos", not(debug_assertions)))]
use anyhow::anyhow;
#[cfg(all(target_os = "macos", not(debug_assertions)))]
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

#[cfg(all(target_os = "macos", not(debug_assertions)))]
const SERVICE: &str = "nordenvault-desktop";
const ACCESS_TOKEN_ACCOUNT: &str = "access_token";
const REFRESH_TOKEN_ACCOUNT: &str = "refresh_token";
const S3_SECRET_ACCOUNT: &str = "s3_secret_key";

// ── file-based helpers (used on Linux, Windows, and debug builds) ─────────

fn secrets_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("nordenvault")
        .join("dev_secrets.json")
}

fn file_read(key: &str) -> Option<String> {
    let path = secrets_path();
    let data = std::fs::read_to_string(&path).ok()?;
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&data).ok()?;
    map.get(key)?.as_str().map(String::from)
}

fn file_write(key: &str, value: &str) -> Result<()> {
    let path = secrets_path();
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

fn file_delete(key: &str) {
    let path = secrets_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(mut map) =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&data)
        {
            map.remove(key);
            let _ = std::fs::write(&path, serde_json::to_string_pretty(&map).unwrap_or_default());
        }
    }
}

// ── Condition: use Keychain on macOS release, file storage otherwise ──────

/// Returns true when we should use the macOS Keychain.
#[cfg(all(target_os = "macos", not(debug_assertions)))]
const USE_KEYCHAIN: bool = true;
#[cfg(not(all(target_os = "macos", not(debug_assertions))))]
const USE_KEYCHAIN: bool = false;

// ── public API ────────────────────────────────────────────────────────────

pub fn store_tokens(access: &str, refresh: &str) -> Result<()> {
    if !USE_KEYCHAIN {
        file_write(ACCESS_TOKEN_ACCOUNT, access)?;
        file_write(REFRESH_TOKEN_ACCOUNT, refresh)?;
        return Ok(());
    }
    #[cfg(all(target_os = "macos", not(debug_assertions)))]
    {
        set_generic_password(SERVICE, ACCESS_TOKEN_ACCOUNT, access.as_bytes())
            .map_err(|e| anyhow!("Failed to store access token: {}", e))?;
        set_generic_password(SERVICE, REFRESH_TOKEN_ACCOUNT, refresh.as_bytes())
            .map_err(|e| anyhow!("Failed to store refresh token: {}", e))?;
    }
    Ok(())
}

pub fn load_tokens() -> Option<(String, String)> {
    if !USE_KEYCHAIN {
        let access = file_read(ACCESS_TOKEN_ACCOUNT)?;
        let refresh = file_read(REFRESH_TOKEN_ACCOUNT)?;
        return Some((access, refresh));
    }
    #[cfg(all(target_os = "macos", not(debug_assertions)))]
    {
        let access = get_generic_password(SERVICE, ACCESS_TOKEN_ACCOUNT).ok()?;
        let refresh = get_generic_password(SERVICE, REFRESH_TOKEN_ACCOUNT).ok()?;
        return Some((
            String::from_utf8(access).ok()?,
            String::from_utf8(refresh).ok()?,
        ));
    }
    #[cfg(not(all(target_os = "macos", not(debug_assertions))))]
    None
}

pub fn delete_tokens() {
    if !USE_KEYCHAIN {
        file_delete(ACCESS_TOKEN_ACCOUNT);
        file_delete(REFRESH_TOKEN_ACCOUNT);
        return;
    }
    #[cfg(all(target_os = "macos", not(debug_assertions)))]
    {
        let _ = delete_generic_password(SERVICE, ACCESS_TOKEN_ACCOUNT);
        let _ = delete_generic_password(SERVICE, REFRESH_TOKEN_ACCOUNT);
    }
}

pub fn store_s3_secret(secret: &str) -> Result<()> {
    if !USE_KEYCHAIN {
        return file_write(S3_SECRET_ACCOUNT, secret);
    }
    #[cfg(all(target_os = "macos", not(debug_assertions)))]
    {
        set_generic_password(SERVICE, S3_SECRET_ACCOUNT, secret.as_bytes())
            .map_err(|e| anyhow!("Failed to store S3 secret: {}", e))?;
    }
    Ok(())
}

pub fn load_s3_secret() -> Option<String> {
    if !USE_KEYCHAIN {
        return file_read(S3_SECRET_ACCOUNT);
    }
    #[cfg(all(target_os = "macos", not(debug_assertions)))]
    {
        let bytes = get_generic_password(SERVICE, S3_SECRET_ACCOUNT).ok()?;
        return String::from_utf8(bytes).ok();
    }
    #[cfg(not(all(target_os = "macos", not(debug_assertions))))]
    None
}

pub fn delete_s3_secret() {
    if !USE_KEYCHAIN {
        file_delete(S3_SECRET_ACCOUNT);
        return;
    }
    #[cfg(all(target_os = "macos", not(debug_assertions)))]
    {
        let _ = delete_generic_password(SERVICE, S3_SECRET_ACCOUNT);
    }
}
