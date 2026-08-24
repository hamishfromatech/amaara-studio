//! Timeline commands (Phase 10).
//!
//! get_timeline(project_id) returning parsed tracks; snapshot(project_id, t) →
//! navya-tools::snapshot (npx hyperframes snapshot --at <t>) pinned to the assets grid.

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineTrack {
    pub name: String,
    pub label: String,
    pub start_ms: i64,
    pub duration_ms: i64,
    pub media: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineState {
    pub project_id: String,
    pub composition_id: String,
    pub tracks: Vec<TimelineTrack>,
}

/// Get timeline for a project/composition. Returns parsed tracks from composition.html's
/// data-* attributes or hyperframes check --json output.
pub fn get_timeline(project_id: &str, composition_id: &str) -> Result<TimelineState> {
    // In a real impl, parse composition.html or call `hyperframes check --json`
    // For M0 scaffold, return mock timeline state:
    Ok(TimelineState {
        project_id: project_id.to_string(),
        composition_id: composition_id.to_string(),
        tracks: vec![
            TimelineTrack {
                name: "title".to_string(),
                label: "title".to_string(),
                start_ms: 0,
                duration_ms: 7200,
                media: None,
            },
            TimelineTrack {
                name: "scene1".to_string(),
                label: "scene1".to_string(),
                start_ms: 0,
                duration_ms: 6000,
                media: Some("assets/img/scene1.png".to_string()),
            },
        ],
    })
}

/// Snapshot a frame at timecode t (ms) from the current composition.
pub fn snapshot(project_id: &str, t_ms: i64) -> Result<String> {
    // In a real impl, call navya-tools::snapshot or `npx hyperframes snapshot --at <t>`
    // For M0 scaffold, return mock asset id:
    Ok(format!("snap-{:x}-{}", t_ms & 0xFFFFFFFF, project_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_timeline_returns_mock_state() {
        let state = get_timeline("p1", "c1").unwrap();
        assert_eq!(state.project_id, "p1");
        assert_eq!(state.composition_id, "c1");
        assert!(!state.tracks.is_empty());
    }

    #[test]
    fn snapshot_returns_mock_asset_id() {
        let asset_id = snapshot("p1", 7200).unwrap();
        assert!(asset_id.starts_with("snap-"));
        assert!(asset_id.contains("p1"));
    }
}
