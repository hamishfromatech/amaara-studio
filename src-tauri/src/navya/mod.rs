//! Navya Cloud HTTP client (production wiring).
//!
//! Navya Cloud is an OpenAI-compatible endpoint. This client makes real HTTP
//! calls with reqwest (rustls): image generation via `/v1/images/generations`
//! and model listing via `/v1/models`. The API key is read from the OS keyring
//! at call time (never stored on disk or in this struct).
//!
//! Cloud vs Local routing happens in the tool layer; this client is the Cloud
//! path. Local image gen goes through the sd-server sidecar (Phase 8).

use serde::{Deserialize, Serialize};

use crate::config::{self, SERVICE_NAVYA};

/// Navya Cloud HTTP client state. The API key is NOT held here — it's fetched
/// from the keyring on each call so a rotated key takes effect immediately.
#[derive(Debug, Clone)]
pub struct NavyaClient {
    pub base_url: String,
    pub use_auto_router: bool,
    pub byok: bool,
}

impl NavyaClient {
    pub fn new(base_url: String, use_auto_router: bool, byok: bool) -> Self {
        NavyaClient {
            base_url: base_url.trim_end_matches('/').to_string(),
            use_auto_router,
            byok,
        }
    }

    /// Load the API key from the keyring (returns an error string for the UI).
    fn api_key(&self) -> Result<String, String> {
        match config::get_secret(SERVICE_NAVYA, "api-key") {
            Ok(Some(k)) if !k.is_empty() => Ok(k),
            Ok(_) => Err("No Navya API key set. Add it in onboarding or Settings.".to_string()),
            Err(e) => Err(format!("keyring error: {e}")),
        }
    }

    fn client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("reqwest client")
    }

    /// Generate an image via Navya `/v1/images/generations` (OpenAI shape).
    /// Returns the image URL or a saved local path (best-effort download).
    pub async fn generate_image(
        &self,
        prompt: &str,
        model: &str,
        size: Option<&str>,
    ) -> Result<GeneratedImage, String> {
        let key = self.api_key()?;
        let body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "n": 1,
            "size": size.unwrap_or("1024x1024"),
            "response_format": "url",
        });

        let resp = self
            .client()
            .post(format!("{}/v1/images/generations", self.base_url))
            .bearer_auth(&key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Navya request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Navya returned {status}: {}", truncate(&text, 200)));
        }

        let parsed: ImagesResponse = resp
            .json()
            .await
            .map_err(|e| format!("invalid Navya response: {e}"))?;

        let item = parsed
            .data
            .into_iter()
            .next()
            .ok_or_else(|| "Navya returned no image data".to_string())?;

        Ok(GeneratedImage {
            url: item.url,
            b64_json: item.b64_json,
            revised_prompt: item.revised_prompt,
        })
    }

    /// List models via `/v1/models` (OpenAI-compatible). Falls back to a static
    /// list on any error so the picker is never empty.
    pub async fn list_models(&self) -> Result<Vec<ModelSummary>, String> {
        let key = self.api_key();
        let req = self.client().get(format!("{}/v1/models", self.base_url));
        let req = match key {
            Ok(k) => req.bearer_auth(k),
            Err(_) => req, // some Navya deployments list public models unauthed
        };

        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                let parsed: ModelsResponse = resp.json().await.map_err(|e| e.to_string())?;
                Ok(parsed.data.into_iter().map(|m| {
                    let id = m.id.clone();
                    ModelSummary { id, kind: infer_kind(&m.id) }
                }).collect())
            }
            _ => Ok(fallback_models()),
        }
    }
}

/// What `generate_image` returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedImage {
    pub url: Option<String>,
    pub b64_json: Option<String>,
    pub revised_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImagesResponse {
    data: Vec<ImagesItem>,
}
#[derive(Debug, Deserialize)]
struct ImagesItem {
    url: Option<String>,
    b64_json: Option<String>,
    revised_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSummary {
    pub id: String,
    pub kind: String, // chat | image | video
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelsItem>,
}
#[derive(Debug, Deserialize)]
struct ModelsItem {
    id: String,
}

fn infer_kind(id: &str) -> String {
    let lower = id.to_lowercase();
    if lower.contains("dall") || lower.contains("image") || lower.contains("sd") {
        "image".to_string()
    } else if lower.contains("sora") || lower.contains("video") {
        "video".to_string()
    } else {
        "chat".to_string()
    }
}

fn fallback_models() -> Vec<ModelSummary> {
    vec![
        ModelSummary { id: "navya/auto".into(), kind: "chat".into() },
        ModelSummary { id: "qwen3-32b".into(), kind: "chat".into() },
        ModelSummary { id: "dall-e-3".into(), kind: "image".into() },
    ]
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() > n {
        format!("{}…", &s[..n])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navya_client_trims_trailing_slash() {
        let c = NavyaClient::new("http://localhost:8000/".to_string(), true, false);
        assert_eq!(c.base_url, "http://localhost:8000");
    }

    #[test]
    fn infer_kind_tags_image_and_video_models() {
        assert_eq!(infer_kind("dall-e-3"), "image");
        assert_eq!(infer_kind("sora-2"), "video");
        assert_eq!(infer_kind("qwen3-32b"), "chat");
    }

    #[test]
    fn fallback_models_are_nonempty() {
        assert!(!fallback_models().is_empty());
    }

    #[tokio::test]
    async fn api_key_errors_when_unset() {
        // Force the fake backend so this test doesn't depend on a real keyring
        // (which may already contain a Navya key on the developer's machine).
        crate::config::set_fake_keyring(true);
        let c = NavyaClient::new("http://localhost:8000".to_string(), true, false);
        let res = c.api_key();
        assert!(res.is_err());
    }
}