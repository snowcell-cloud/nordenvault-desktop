use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use aws_config::Region;
use aws_credential_types::Credentials;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use tokio::sync::Mutex;

use crate::auth::keychain;
use crate::config::AgentConfig;
use crate::state::AgentStatus;
use crate::upload::queue::UploadQueue;

pub async fn run_upload_worker(
    queue: Arc<UploadQueue>,
    status: Arc<Mutex<AgentStatus>>,
    config: Arc<Mutex<AgentConfig>>,
    paused: Arc<AtomicBool>,
) {
    let mut s3_client: Option<(S3Client, String)> = None; // (client, bucket)

    loop {
        let job = queue.pop().await;

        if paused.load(Ordering::Relaxed) {
            queue.push(job).await;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }

        // Build (or reuse) the S3 client. Only rebuild when we don't have one yet.
        if s3_client.is_none() {
            match build_s3_client(&config).await {
                Ok(c) => {
                    let bucket = config.lock().await.bucket_name.clone().unwrap_or_default();
                    s3_client = Some((c, bucket));
                    status.lock().await.error_message = None;
                }
                Err(e) => {
                    let mut s = status.lock().await;
                    s.status = "error".into();
                    s.error_message = Some(format!("Storage not configured: {e}"));
                    // Re-enqueue the job and back off — don't spin.
                    queue.push(job).await;
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    continue;
                }
            }
        }

        let (s3, bucket) = s3_client.as_ref().unwrap();

        {
            let mut s = status.lock().await;
            s.status = "syncing".into();
        }

        let result = upload_file(s3, bucket, &job.local_path, &job.s3_key).await;

        match result {
            Ok(_) => {
                let mut s = status.lock().await;
                s.last_backup_at = Some(chrono::Utc::now().to_rfc3339());
                s.queue_depth = queue.depth().await;
                if s.queue_depth == 0 {
                    s.status = "idle".into();
                }
            }
            Err(e) => {
                eprintln!("[upload] upload failed for {}: {e:#}", job.s3_key);
                let mut s = status.lock().await;
                s.status = "error".into();
                s.error_message = Some(e.to_string());
                s3_client = None;
                queue.push(job).await;
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        }
    }
}

async fn build_s3_client(config: &Arc<Mutex<AgentConfig>>) -> Result<S3Client> {
    let cfg = config.lock().await;
    let access_key_id = cfg
        .access_key_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("No S3 access key configured"))?;
    let secret = keychain::load_s3_secret()
        .ok_or_else(|| anyhow::anyhow!("No S3 secret in keychain"))?;
    let endpoint = cfg
        .endpoint_url
        .clone()
        .ok_or_else(|| anyhow::anyhow!("No S3 endpoint configured"))?;
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
