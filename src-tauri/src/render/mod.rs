//! Render sidecar + queue UI (Phase 7).
//!
//! Real video rendering via a Node 22 sidecar running `npx hyperframes` /
//! `@remotion/renderer`, the `render_to_video` tool, the SQLite-backed render queue,
//! and the Renders tabs + right-rail queue.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Render quality options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RenderQuality {
    #[serde(alias = "draft")]
    Draft,
    #[serde(alias = "high")]
    High,
}

impl Default for RenderQuality {
    fn default() -> Self {
        RenderQuality::Draft
    }
}

/// Render target options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RenderTarget {
    #[serde(alias = "local")]
    Local,
    #[serde(alias = "docker")]
    Docker,
    #[serde(alias = "cloud")]
    Cloud,
    #[serde(alias = "lambda")]
    Lambda,
    #[serde(alias = "cloudrun")]
    CloudRun,
}

impl Default for RenderTarget {
    fn default() -> Self {
        RenderTarget::Local
    }
}

/// A render job in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderJob {
    pub job_id: String,
    pub project_id: String,
    pub composition_id: String,
    pub target: RenderTarget,
    pub quality: RenderQuality,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub status: RenderStatus,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub output_path: Option<PathBuf>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RenderStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl Default for RenderJob {
    fn default() -> Self {
        RenderJob {
            job_id: "default".to_string(),
            project_id: "default".to_string(),
            composition_id: "default".to_string(),
            target: RenderTarget::default(),
            quality: RenderQuality::Draft,
            width: 1280,
            height: 720,
            fps: 30,
            status: RenderStatus::Queued,
            started_at_ms: None,
            finished_at_ms: None,
            output_path: None,
            error: None,
        }
    }
}

/// In-memory render queue (Phase 7 scaffold). Real implementation uses SQLite
/// via the store module for persistence across restarts.
#[derive(Default)]
pub struct RenderQueue {
    jobs: std::collections::HashMap<String, RenderJob>,
}

impl RenderQueue {
    pub fn new() -> Self {
        RenderQueue {
            jobs: std::collections::HashMap::new(),
        }
    }

    pub fn add(&mut self, job: RenderJob) {
        self.jobs.insert(job.job_id.clone(), job);
    }

    pub fn get(&self, job_id: &str) -> Option<&RenderJob> {
        self.jobs.get(job_id)
    }

    pub fn list(&self) -> Vec<RenderJob> {
        let mut jobs: Vec<_> = self.jobs.values().cloned().collect();
        // Sort by status: queued/running first, then done/failed/cancelled
        jobs.sort_by(|a, b| a.status.cmp(&b.status));
        jobs
    }

    pub fn update_status(&mut self, job_id: &str, status: RenderStatus) -> bool {
        if let Some(job) = self.jobs.get_mut(job_id) {
            job.status = status;
            if status == RenderStatus::Running && job.started_at_ms.is_none() {
                job.started_at_ms = Some(current_time_ms());
            }
            if status == RenderStatus::Done || status == RenderStatus::Failed || status == RenderStatus::Cancelled {
                job.finished_at_ms = Some(current_time_ms());
            }
            true
        } else {
            false
        }
    }
}

fn current_time_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_queue_add_and_list() {
        let mut queue = RenderQueue::new();
        let job = RenderJob {
            job_id: "r1".to_string(),
            ..RenderJob::default()
        };
        queue.add(job);
        let list = queue.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].job_id, "r1");
    }

    #[test]
    fn update_status_works() {
        let mut queue = RenderQueue::new();
        let job = RenderJob {
            job_id: "r2".to_string(),
            ..RenderJob::default()
        };
        queue.add(job);
        assert!(queue.update_status("r2", RenderStatus::Running));
        let j = queue.get("r2").unwrap();
        assert_eq!(j.status, RenderStatus::Running);
        assert!(j.started_at_ms.is_some());
    }
}
