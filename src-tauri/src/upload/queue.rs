use std::collections::VecDeque;
use std::path::PathBuf;
use tokio::sync::{Mutex, Notify};

#[derive(Debug, Clone)]
pub struct UploadJob {
    pub local_path: PathBuf,
    pub s3_key: String,
}

pub struct UploadQueue {
    inner: Mutex<VecDeque<UploadJob>>,
    notify: Notify,
}

impl UploadQueue {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
        }
    }

    pub async fn push(&self, job: UploadJob) {
        self.inner.lock().await.push_back(job);
        self.notify.notify_one();
    }

    pub async fn pop(&self) -> UploadJob {
        loop {
            {
                let mut q = self.inner.lock().await;
                if let Some(job) = q.pop_front() {
                    return job;
                }
            }
            self.notify.notified().await;
        }
    }

    pub async fn depth(&self) -> usize {
        self.inner.lock().await.len()
    }

    /// Remove all queued jobs whose local path starts with the given prefix.
    pub async fn remove_prefix(&self, prefix: &std::path::Path) {
        let mut q = self.inner.lock().await;
        q.retain(|job| !job.local_path.starts_with(prefix));
    }
}
