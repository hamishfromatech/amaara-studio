//! Navya Studio shared tool-backend lib (Phase 3).
//!
//! Pure-ish async fns that take a `ToolContext`. No transport code (HTTP/MCP) here —
//! those live in the control server (`src-tauri/src/control/`) and MCP bindings.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Non-secret config view passed to tool fns.
#[derive(Debug, Clone)]
pub struct ConfigRef {
    pub navya_base_url: String,
    pub default_model: String,
    pub use_auto_router: bool,
    pub byok: bool,
    pub enabled_harnesses: Vec<String>,
    pub local_llama_url: String,
}

/// Minimal store trait for tool fns to query/insert project/asset data.
pub trait StoreRead {
    fn list_projects(&self) -> Result<Vec<ProjectRow>, anyhow::Error>;
    fn insert_asset(
        &self,
        project_id: &str,
        composition_id: Option<&str>,
        path: &str,
        kind: &str,
        source: &str,
        prompt: Option<&str>,
    ) -> Result<String, anyhow::Error>; // returns asset id
}

/// Sidecar status enum (shared with src-tauri sidecar module).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarStatus {
    Starting,
    Running,
    Stopped,
    Exited(i32),
}

/// Minimal sidecar supervisor view for tool fns to query status.
pub trait SidecarView {
    fn status(&self, name: &str) -> Option<SidecarStatus>;
}

/// Project row reference (matches store::ProjectRow shape).
#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub dir: String,
    pub created_at_ms: i64,
    pub harness: String,
    pub model: String,
    pub source: String,
}

/// Context passed to all tool functions. Owned or borrowed as needed.
#[allow(missing_debug_implementations)]
pub struct ToolContext<'c> {
    pub project_dir: PathBuf,
    pub config: &'c ConfigRef,
    pub store: &'c dyn StoreRead,
    pub sidecars: &'c dyn SidecarView,
}

/// --- Tool Request/Response Types ------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateImageReq {
    pub project_id: String,
    pub composition_id: Option<String>,
    pub prompt: String,
    pub model: Option<String>,
    pub size: Option<String>, // e.g. "1024x1024"
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratedImage {
    pub asset_id: String,
    pub path: PathBuf,
    pub prompt: String,
    pub model_used: String,
    pub source: String, // "cloud" or "local"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderToVideoReq {
    pub project_id: String,
    pub composition_id: String,
    pub quality: String, // "draft" or "high"
    pub target: String,  // "local", "docker", "cloud", "lambda", "cloudrun"
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderJob {
    pub job_id: String,
    pub project_id: String,
    pub composition_id: String,
    pub target: String,
    pub quality: String,
    pub status: String, // "queued", "running", "done", "failed"
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelList {
    pub cloud_models: Vec<String>,
    pub local_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectState {
    pub project_id: String,
    pub name: String,
    pub harness: String,
    pub model: String,
    pub source: String,
    pub compositions_count: usize,
    pub assets_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub asset_id: String,
    pub path: PathBuf,
    pub timecode_ms: i64,
}

/// --- Tool Functions -------------------------------------------------------------

/// Generate an image. Routes to cloud or local based on ctx.config and source preference.
/// Returns GeneratedImage with asset record created in the store.
pub async fn generate_image(ctx: &ToolContext<'_>, req: GenerateImageReq) -> anyhow::Result<GeneratedImage> {
    // In a real impl, this would call Navya Cloud /v1/images/generations or sd-server.
    // For M0 scaffold, we return a stub generated image with a mock asset path.
    let model_used = req.model.clone().unwrap_or_else(|| ctx.config.default_model.clone());
    let source = if model_used.starts_with("sd-") || model_used.contains("local") {
        "local".to_string()
    } else {
        "cloud".to_string()
    };

    // Generate a mock asset id and path.
    let asset_id = format!("img-{}", uuid_or_hash(&req.prompt));
    let asset_path = ctx.project_dir.join("assets").join("img").join(format!("{}.png", asset_id));

    // Insert into store (mock successful insert).
    ctx.store.insert_asset(
        &req.project_id,
        req.composition_id.as_ref().map(|s| s.as_str()),
        &asset_path.to_string_lossy(),
        "image",
        &source,
        Some(&req.prompt),
    )?;

    let source_str = source.clone();
    Ok(GeneratedImage {
        asset_id,
        path: asset_path,
        prompt: req.prompt,
        model_used,
        source: source_str.clone(),
        cost_usd: if source == "cloud" { Some(0.002) } else { None },
    })
}

/// Render to video via the render queue / sidecar. Returns a RenderJob id.
pub async fn render_to_video(ctx: &ToolContext<'_>, req: RenderToVideoReq) -> anyhow::Result<RenderJob> {
    let job_id = format!("render-{}", uuid_or_hash(&format!("{}-{}", req.project_id, req.composition_id)));
    
    Ok(RenderJob {
        job_id,
        project_id: req.project_id,
        composition_id: req.composition_id,
        target: req.target,
        quality: req.quality,
        status: "queued".to_string(),
    })
}

/// List available models (cloud + local). Mock implementation for M0.
pub async fn list_local_models(_ctx: &ToolContext<'_>) -> anyhow::Result<ModelList> {
    Ok(ModelList {
        cloud_models: vec!["navya/auto".to_string(), "qwen3-32b".to_string()],
        local_models: vec!["llama3-8b".to_string(), "sd-xl".to_string()],
    })
}

/// Set the generation source (cloud vs local). No-op for M0 scaffold.
pub async fn set_generation_source(_ctx: &ToolContext<'_>, _source: String) -> anyhow::Result<()> {
    Ok(())
}

/// Get current project state from store + config.
pub async fn get_project_state(ctx: &ToolContext<'_>) -> anyhow::Result<ProjectState> {
    let projects = ctx.store.list_projects()?;
    let first = projects.first().map(|p| p.id.clone()).unwrap_or_else(|| "default".to_string());
    let name = projects.first().map(|p| p.name.clone()).unwrap_or_else(|| "default-project".to_string());

    Ok(ProjectState {
        project_id: first,
        name,
        harness: ctx.config.enabled_harnesses.first().cloned().unwrap_or("aaa-coder-cli".to_string()),
        model: ctx.config.default_model.clone(),
        source: "cloud".to_string(), // default per A.3
        compositions_count: 0,       // mock
        assets_count: 0,             // mock
    })
}

/// Snapshot a frame at timecode t (ms) from the current composition. Mock for M0.
pub async fn snapshot(ctx: &ToolContext<'_>, t_ms: i64) -> anyhow::Result<Snapshot> {
    let asset_id = format!("snap-{}", uuid_or_hash(&format!("{}", t_ms)));
    let path = ctx.project_dir.join("assets").join("img").join(format!("snapshot-{}.png", asset_id));

    ctx.store.insert_asset(
        &"default".to_string(),
        None,
        &path.to_string_lossy(),
        "image",
        "cloud",
        Some(&format!("snapshot at {}ms", t_ms)),
    )?;

    Ok(Snapshot {
        asset_id,
        path,
        timecode_ms: t_ms,
    })
}

/// Simple hash/uuid mock for unique ids.
fn uuid_or_hash(s: &str) -> String {
    let mut h: u32 = 0;
    for c in s.chars() {
        h = h.wrapping_mul(31).wrapping_add(c as u32);
    }
    format!("{:08x}", h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_context_serializes() {
        // ToolContext is not serialized (has &'c dyn), but request/response types are.
        let req = GenerateImageReq {
            project_id: "p1".to_string(),
            composition_id: Some("c1".to_string()),
            prompt: "test".to_string(),
            model: None,
            size: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: GenerateImageReq = serde_json::from_str(&json).unwrap();
        assert_eq!(back.project_id, "p1");
    }

    #[test]
    fn uuid_or_hash_is_deterministic() {
        let h1 = uuid_or_hash("black-holes-explainer");
        let h2 = uuid_or_hash("black-holes-explainer");
        assert_eq!(h1, h2);
    }
}
