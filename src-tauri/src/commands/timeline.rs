//! Timeline commands (Phase 10).
//!
//! - `get_timeline` discovers the project's composition HTML and parses it via
//!   `crate::timeline` — the real `data-*` parser that reads a HyperFrames
//!   composition's timing (replacing the M0 mock that returned hand-written
//!   tracks).
//! - `snapshot` pins a frame at a timecode by running
//!   `npx hyperframes snapshot --at <t>` in the project dir.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

/// Pinned snapshot result returned to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotResult {
    pub asset_id: String,
    pub path: PathBuf,
    pub timecode_ms: i64,
}

/// Look up a project's on-disk directory from the store.
fn project_dir_for(
    state: &std::sync::Arc<crate::state::AppState>,
    project_id: &str,
) -> Result<PathBuf, String> {
    let store = state.store.lock();
    let projects = store.list_projects().map_err(|e| e.to_string())?;
    let proj = projects
        .into_iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("project {project_id} not found"))?;
    Ok(PathBuf::from(proj.dir))
}

/// Get the parsed timeline (clips + tracks) for a project's composition.
#[tauri::command]
pub async fn get_timeline(
    state: State<'_, std::sync::Arc<crate::state::AppState>>,
    project_id: String,
    composition_id: String,
) -> Result<crate::timeline::TimelineState, String> {
    let dir = project_dir_for(&state, &project_id)?;
    let mut parsed = crate::timeline::load_timeline(&dir, &composition_id)
        .map_err(|e| e.to_string())?;
    parsed.project_id = project_id;
    Ok(parsed)
}

/// Snapshot a frame at timecode `t_ms` (milliseconds) from the composition.
///
/// Runs `npx hyperframes snapshot --at <secs> --output <project>/snapshots` in
/// the project dir and returns the most recently produced PNG.
#[tauri::command]
pub async fn snapshot(
    app: AppHandle,
    state: State<'_, std::sync::Arc<crate::state::AppState>>,
    project_id: String,
    t_ms: i64,
) -> Result<SnapshotResult, String> {
    snapshot_core(&app, &state, &project_id, t_ms).await
}

/// Core snapshot shared by the UI command and the control-server tool dispatch.
pub async fn snapshot_core(
    app: &AppHandle,
    state: &std::sync::Arc<crate::state::AppState>,
    project_id: &str,
    t_ms: i64,
) -> Result<SnapshotResult, String> {
    let dir = project_dir_for(state, project_id)?;

    let secs = (t_ms as f64 / 1000.0).round();
    let output_dir = dir.join("snapshots");
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("creating snapshots dir: {e}"))?;

    // `npx` on Windows resolves npx.cmd automatically through the runtime.
    let mut cmd = tokio::process::Command::new("npx");
    cmd.arg("hyperframes")
        .arg("snapshot")
        .arg("--at")
        .arg(format!("{secs}"))
        .arg("--output")
        .arg(&output_dir)
        .arg("--no-telemetry")
        .current_dir(&dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("failed to run hyperframes snapshot: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "hyperframes snapshot failed (exit {}):{}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    // Find the newest PNG in the output dir.
    let png = newest_png(&output_dir)
        .ok_or_else(|| "hyperframes snapshot produced no PNG output".to_string())?;

    let asset_id = format!(
        "snap-{:x}-{}",
        (t_ms & 0xFFFF_FFFF) as u32,
        project_id
    );

    // Surface the snapshot to the UI so it can be pinned to the Assets grid.
    let _ = app.emit(
        "studio://event",
        crate::events::StudioEvent::Project(crate::events::ProjectEvent::AssetAdded {
            project_id: project_id.to_string(),
            asset_id: asset_id.clone(),
            path: png.to_string_lossy().to_string(),
        }),
    );

    Ok(SnapshotResult {
        asset_id,
        path: png,
        timecode_ms: t_ms,
    })
}

/// Return the newest PNG file in `dir`, if any.
fn newest_png(dir: &Path) -> Option<PathBuf> {
    let mut entries: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let read = std::fs::read_dir(dir).ok()?;
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("png") {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    entries.push((modified, path));
                }
            }
        }
    }
    entries.sort_by_key(|b| std::cmp::Reverse(b.0));
    entries.into_iter().next().map(|(_, p)| p)
}

/// Pure parse helper kept for direct unit testing of the command layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSummary {
    pub clips: usize,
    pub tracks: usize,
    pub duration_ms: i64,
}

/// Summarize a composition's HTML without the full timeline (cheap preview).
pub fn summarize_composition(html: &str) -> Result<TimelineSummary, String> {
    let clips = crate::timeline::parse_composition_html(html);
    let (tracks, duration) = crate::timeline::clips_to_tracks(&clips);
    Ok(TimelineSummary {
        clips: clips.len(),
        tracks: tracks.len(),
        duration_ms: duration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_reads_sample_html() {
        let html = r#"<div class="clip" data-composition-id="title"
             data-composition-src="blocks/title.html" data-start="0"
             data-duration="6" data-track-index="0"></div>
             <div class="clip" data-composition-id="s1"
             data-composition-src="a.png" data-start="0"
             data-duration="12" data-track-index="1"></div>"#;
        let s = summarize_composition(html).unwrap();
        assert_eq!(s.clips, 2);
        assert_eq!(s.tracks, 2);
        assert_eq!(s.duration_ms, 12000);
    }

    #[test]
    fn summarize_empty_when_no_clips() {
        let s = summarize_composition("<div class='x'>hi</div>").unwrap();
        assert_eq!(s.clips, 0);
        assert_eq!(s.tracks, 0);
    }
}
