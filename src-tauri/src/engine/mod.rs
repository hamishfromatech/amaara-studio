//! Amaara Engine client (local llama.cpp proxy).
//!
//! Discovers and talks to the Amaara Engine OpenAI-compatible HTTP proxy
//! running on localhost (default port 7685). The Engine aggregates models
//! from multiple local backends (llama.cpp, FastFlowLM, MeshLLM, vLLM).
//!
//! Endpoints consumed:
//!   GET  /health          -> liveness probe
//!   GET  /v1/models       -> list available local models
//!   POST /v1/chat/completions  -> chat completions (future direct use)

use serde::{Deserialize, Serialize};

const DEFAULT_ENGINE_URL: &str = "http://127.0.0.1:7685";

/// Client for the Amaara Engine local proxy.
#[derive(Debug, Clone)]
pub struct EngineClient {
    base_url: String,
    client: reqwest::Client,
}

impl Default for EngineClient {
    fn default() -> Self {
        Self::new(DEFAULT_ENGINE_URL)
    }
}

impl EngineClient {
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        }
    }

    /// Quick liveness probe (should return within ~1s).
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/health", self.base_url);
        match self.client.get(&url).send().await {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        }
    }

    /// List models from the Engine proxy.
    /// Returns empty Vec when Engine is offline.
    pub async fn list_models(&self) -> Vec<EngineModel> {
        let url = format!("{}/v1/models", self.base_url);
        let response = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        if !response.status().is_success() {
            return Vec::new();
        }
        let body: ModelsResponse = match response.json().await {
            Ok(b) => b,
            Err(_) => return Vec::new(),
        };
        body.data.into_iter().map(Into::into).collect()
    }

    /// Format a model ID the way the a-coder-cli harness expects it for a
    /// local OpenAI-compatible endpoint: "openai/<model-id>" (provider/model).
    pub fn format_model_id(model_id: &str) -> String {
        format!("openai/{model_id}")
    }
}

/// A model surfaced by the Engine proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineModel {
    pub id: String,
    pub name: String, // human-friendly (derived from id + meta)
    pub aliases: Vec<String>,
    pub size_bytes: Option<u64>,
    pub param_count: Option<u64>,
    pub context_length: Option<u32>,
    pub owned_by: String,
}

/// OpenAI /v1/models response shape.
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<RawModel>,
}

/// Individual model entry from Engine.
#[derive(Debug, Deserialize)]
struct RawModel {
    id: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    meta: Option<ModelMeta>,
    #[serde(default)]
    owned_by: String,
}

#[derive(Debug, Deserialize, Default)]
struct ModelMeta {
    #[serde(default)]
    size: Option<u64>,
    #[serde(default, rename = "n_params")]
    n_params: Option<u64>,
    #[serde(default, rename = "n_ctx")]
    n_ctx: Option<u32>,
    #[serde(default)]
    ftype: Option<String>,
}

impl From<RawModel> for EngineModel {
    fn from(r: RawModel) -> Self {
        let meta = r.meta.unwrap_or_default();
        let name = if let Some(ref ft) = meta.ftype {
            format!("{} ({})", r.id, ft)
        } else {
            r.id.clone()
        };
        EngineModel {
            id: r.id,
            name,
            aliases: r.aliases,
            size_bytes: meta.size,
            param_count: meta.n_params,
            context_length: meta.n_ctx,
            owned_by: r.owned_by,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_models_response() {
        let json = r#"{"object":"list","data":[{"id":"qwen-coder","object":"model","owned_by":"llamacpp","aliases":["qwen-coder"],"meta":{"ftype":"IQ2_M - 2.7 bpw","n_ctx":262144,"n_ctx_train":262144,"n_embd":2048,"n_params":35505251456,"n_vocab":248320,"size":11871977984,"vocab_type":2}}]}"#;
        let body: ModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(body.data.len(), 1);
        let m: EngineModel = body.data.into_iter().next().unwrap().into();
        assert_eq!(m.id, "qwen-coder");
        assert_eq!(m.owned_by, "llamacpp");
        assert_eq!(m.param_count, Some(35505251456));
        assert_eq!(m.context_length, Some(262144));
    }

    #[test]
    fn format_model_id_for_harness() {
        assert_eq!(
            EngineClient::format_model_id("qwen-coder"),
            "openai/qwen-coder"
        );
    }
}
