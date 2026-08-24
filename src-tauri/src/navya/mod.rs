//! Navya Cloud HTTP client (Phase 8).
//!
//! Navya Cloud and a local llama.cpp server are just "providers" to the harness.
//! Every harness in the matrix has a model/provider config mechanism, and all of
//! them treat an OpenAI-compatible endpoint as a first-class provider.

use serde::{Deserialize, Serialize};

/// Navya Cloud HTTP client state (Phase 8 scaffold).
#[derive(Debug, Clone)]
pub struct NavyaClient {
    pub base_url: String,
    pub api_key_secret: Option<String>, // stored in keyring, never on disk
    pub use_auto_router: bool,
    pub byok: bool,
}

impl NavyaClient {
    pub fn new(base_url: String, use_auto_router: bool, byok: bool) -> Self {
        NavyaClient {
            base_url,
            api_key_secret: None, // loaded from keyring at runtime
            use_auto_router,
            byok,
        }
    }

    /// Generate image via Navya /v1/images/generations (OpenAI shape).
    pub async fn generate_image(&self, prompt: &str, model: &str) -> Result<String, String> {
        // In a real impl, POST to /v1/images/generations with:
        // {model, prompt, n, size, quality, response_format}
        // Auth via Authorization: Bearer <key> from keyring.

        // Response is {data:[{url}|{b64_json}]}. Save into assets/img/, write an assets row, return path+meta.

        Ok(format!("assets/img/navya-{}.png", hash_prompt(prompt)))
    }

    /// List models via /public-models or /config endpoints.
    pub async fn list_models(&self) -> Result<Vec<String>, String> {
        // In a real impl, GET /public-models returns {id, name, context_window, kind:"chat"|"image"|"video", ...}
        // Populate the Models list from this at runtime (don't hardcode navya/auto etc.).

        Ok(vec!["navya/auto".to_string(), "qwen3-32b".to_string(), "dall-e-3".to_string()])
    }
}

fn hash_prompt(s: &str) -> String {
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
    fn navya_client_defaults() {
        let client = NavyaClient::new("http://localhost:8000".to_string(), true, false);
        assert_eq!(client.base_url, "http://localhost:8000");
        assert!(client.use_auto_router);
        assert!(!client.byok);
    }

    #[tokio::test]
    async fn list_models_returns_mock_list() {
        let client = NavyaClient::new("http://localhost:8000".to_string(), true, false);
        let models = client.list_models().await.unwrap();
        assert!(models.contains(&"navya/auto".to_string()));
    }
}
