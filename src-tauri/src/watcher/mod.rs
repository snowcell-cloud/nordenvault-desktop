use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};

use crate::config::AgentConfig;
use crate::upload::queue::{UploadJob, UploadQueue};

/// Derive the S3 key for a local file under a watched folder.
/// e.g. s3_prefix="machines/my-macbook", watched="/Users/foo/Documents", file="/Users/foo/Documents/notes/a.txt"
///   -> "machines/my-macbook/Documents/notes/a.txt"
pub fn s3_key_for(s3_prefix: &str, watched_root: &str, file_path: &str) -> String {
    let root = Path::new(watched_root);
    let file = Path::new(file_path);
    let relative = file.strip_prefix(root).unwrap_or(file);
    let folder_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("backup");
    format!(
        "{}/{}/{}",
        s3_prefix,
        folder_name,
        relative.to_string_lossy()
    )
}

pub fn start_watcher(
    config: &AgentConfig,
    queue: Arc<UploadQueue>,
) -> Vec<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>> {
    let s3_prefix = config
        .s3_prefix
        .clone()
        .or_else(|| config.machine_id.map(|id| format!("machines/{}", id)))
        .unwrap_or_else(|| "machines/unknown".into());

    let mut debouncers = vec![];

    for folder in &config.watched_folders {
        if !folder.enabled {
            continue;
        }

        if let Some(d) = start_watcher_for_folder(&s3_prefix, &folder.path, queue.clone()) {
            debouncers.push(d);
        }
    }

    debouncers
}

pub fn start_watcher_for_folder(
    s3_prefix: &str,
    folder_path: &str,
    queue: Arc<UploadQueue>,
) -> Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>> {
    let path = folder_path.to_string();
    let root = path.clone();
    let mid = s3_prefix.to_string();
    let q = queue.clone();

    let debouncer = new_debouncer(
        Duration::from_secs(2),
        move |result: DebounceEventResult| {
            if let Ok(events) = result {
                for event in events {
                    let p = &event.path;
                    if p.is_file() {
                        let file_str = p.to_string_lossy().to_string();
                        let key = s3_key_for(&mid, &root, &file_str);
                        let job = UploadJob {
                            local_path: p.clone(),
                            s3_key: key,
                        };
                        let q2 = q.clone();
                        tauri::async_runtime::spawn(async move {
                            q2.push(job).await;
                        });
                    }
                }
            }
        },
    );

    match debouncer {
        Ok(mut d) => {
            if let Err(e) = d.watcher().watch(Path::new(&path), RecursiveMode::Recursive) {
                eprintln!("Failed to watch {}: {}", path, e);
                None
            } else {
                Some(d)
            }
        }
        Err(e) => {
            eprintln!("Failed to create watcher for {}: {}", path, e);
            None
        }
    }
}

/// Walk `folder_path` recursively and enqueue every existing file for upload.
pub async fn scan_existing_files(s3_prefix: &str, folder_path: &str, queue: Arc<UploadQueue>) {
    let root = folder_path.to_string();
    let mid = s3_prefix.to_string();
    let mut stack = vec![std::path::PathBuf::from(folder_path)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                let file_str = path.to_string_lossy().to_string();
                let key = s3_key_for(&mid, &root, &file_str);
                queue.push(UploadJob { local_path: path, s3_key: key }).await;
            }
        }
    }
}
