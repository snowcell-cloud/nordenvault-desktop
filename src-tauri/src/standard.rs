//! Standard mode uses ordinary S3 objects readable through the NordenVault dashboard.
use crate::{auth::keychain, config::AgentConfig};
use anyhow::{Context, Result};
use aws_config::Region;
use aws_credential_types::Credentials;
use aws_sdk_s3::{primitives::ByteStream, Client as S3Client};
use std::{collections::HashMap, path::PathBuf, time::SystemTime};

#[derive(Default)]
pub struct StandardBackup {
    // Scoped to the registration and folder selection. Only cache successful PUTs.
    files: HashMap<(uuid::Uuid, String, PathBuf), (u64, SystemTime)>,
}
impl StandardBackup {
    pub fn clear(&mut self) {
        self.files.clear();
    }

    pub async fn backup(&mut self, cfg: &AgentConfig) -> Result<()> {
        let bucket = cfg
            .bucket_name
            .as_deref()
            .context("Storage is not configured.")?;
        let prefix = cfg
            .s3_prefix
            .as_deref()
            .context("Missing machine prefix.")?;
        let credential = cfg.credential_id.context("Missing storage credential.")?;
        let client = build_s3_client(cfg).await?;
        let mut current = HashMap::new();
        for folder in cfg.watched_folders.iter().filter(|f| f.enabled) {
            let root = std::path::Path::new(&folder.path);
            let name = root
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("backup");
            for item in walkdir::WalkDir::new(root).follow_links(false) {
                let item = item?;
                if !item.file_type().is_file() {
                    continue;
                }
                let path = item.path();
                let info = item.metadata()?;
                let stamp = (info.len(), info.modified()?);
                let cache_key = (credential, folder.id.clone(), path.to_owned());
                if self.files.get(&cache_key) != Some(&stamp) {
                    let relative = path
                        .strip_prefix(root)?
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/");
                    let key = format!("{prefix}/{name}/{relative}");
                    upload_file(&client, bucket, path, &key).await?;
                    self.files.insert(cache_key.clone(), stamp);
                }
                current.insert(cache_key, stamp);
            }
        }
        self.files = current;
        Ok(())
    }
}
async fn build_s3_client(cfg: &AgentConfig) -> Result<S3Client> {
    let access_key_id = cfg
        .access_key_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("No S3 access key configured"))?;
    let secret =
        keychain::load_s3_secret().ok_or_else(|| anyhow::anyhow!("No S3 secret in keychain"))?;
    let endpoint = cfg
        .endpoint_url
        .clone()
        .ok_or_else(|| anyhow::anyhow!("No S3 endpoint configured"))?;
    anyhow::ensure!(
        url::Url::parse(&endpoint)?.scheme() == "https",
        "Standard backups require HTTPS."
    );
    let region_str = cfg.region.clone().unwrap_or_else(|| "auto".into());

    let creds = Credentials::new(access_key_id, secret, None, None, "nordenvault");
    let sdk_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(Region::new(region_str))
        .credentials_provider(creds)
        .endpoint_url(endpoint)
        .load()
        .await;

    // Force path-style so the SDK uses https://endpoint/bucket/key
    // instead of virtual-hosted https://bucket.endpoint/key
    let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
        .force_path_style(true)
        .build();
    Ok(S3Client::from_conf(s3_config))
}

async fn upload_file(
    s3: &S3Client,
    bucket: &str,
    local_path: &std::path::Path,
    s3_key: &str,
) -> Result<()> {
    let body = ByteStream::from_path(local_path).await?;
    s3.put_object()
        .bucket(bucket)
        .key(s3_key)
        .body(body)
        .send()
        .await?;
    Ok(())
}
