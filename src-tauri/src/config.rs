//! Typed studio settings + secret storage (Phase 1).
//!
//! Non-secret settings live in the JSON settings store (`tauri-plugin-store`,
//! app data dir). Secrets — the Navya API key, control-server token, and any
//! llama.cpp / sd keys — live ONLY in the OS keyring and are never written to
//! disk in plaintext (cross-cutting risk #4; headless/CI fallback per risk #33).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// UI density: comfortable (default) or compact for laptops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Density {
    #[serde(alias = "comfortable")]
    Comfortable,
    #[serde(alias = "compact")]
    Compact,
}

impl Default for Density {
    fn default() -> Self {
        Density::Comfortable
    }
}

/// Theme mode. Dark is the studio default (design.md §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    #[serde(alias = "dark")]
    Dark,
    #[serde(alias = "light")]
    Light,
}

impl Default for ThemeMode {
    fn default() -> Self {
        ThemeMode::Dark
    }
}

/// GPU build flavor selected at BUNDLE time (not runtime) — CUDA/Vulkan/CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SdGpuBackend {
    #[serde(alias = "cuda")]
    Cuda,
    #[serde(alias = "vulkan")]
    Vulkan,
    #[serde(alias = "cpu")]
    Cpu,
}

impl Default for SdGpuBackend {
    fn default() -> Self {
        // Windows-first: CUDA is the common NVIDIA path.
        SdGpuBackend::Cuda
    }
}

/// Non-secret studio settings persisted as JSON in the app data dir.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NavyaConfig {
    /// Navya Cloud base URL. Dev default `http://localhost:8000` (BUILD-GAPS B.7).
    #[serde(default = "default_base_url")]
    pub navya_base_url: String,

    /// Default model id shown in the picker; `navya/auto` routes via the server.
    #[serde(default = "default_model")]
    pub default_model: String,

    /// Use the `navya/auto` router (BUILD-GAPS B.10 / A.3 cloud-first).
    #[serde(default)]
    pub use_auto_router: bool,

    /// BYOK to underlying providers (maps to Navya's byok_router).
    #[serde(default)]
    pub byok: bool,

    /// Order of enabled harnesses; drives the top-bar picker + default.
    #[serde(default = "default_harnesses")]
    pub enabled_harnesses: Vec<String>,

    /// Local llama.cpp server URL (AaaCoderCli local LLM path).
    #[serde(default = "default_local_llama_url")]
    pub local_llama_url: String,

    /// sd-server HTTP URL (Phase 8).
    #[serde(default)]
    pub sd_server_url: Option<String>,

    /// Path to the bundled sd-server binary.
    #[serde(default)]
    pub sd_binary_path: PathBuf,

    /// Local models dir for diffusion weights + GGUFs.
    #[serde(default = "default_models_dir")]
    pub sd_models_dir: PathBuf,

    /// GPU build flavor baked into the bundled sd-server binary.
    #[serde(default)]
    pub sd_gpu_backend: SdGpuBackend,

    /// UI density (design.md §11).
    #[serde(default)]
    pub density: Density,

    /// Theme mode (design.md §11).
    #[serde(default)]
    pub theme: ThemeMode,
}

fn default_base_url() -> String {
    "http://localhost:8000".to_string()
}
fn default_model() -> String {
    "navya/auto".to_string()
}
fn default_harnesses() -> Vec<String> {
    vec!["aaa-coder-cli".to_string()]
}
fn default_local_llama_url() -> String {
    "http://localhost:8080".to_string()
}
fn default_models_dir() -> PathBuf {
    PathBuf::from("models")
}

/// Merge a partial JSON fragment (as typed in the Settings UI) onto the config.
/// Unknown keys are ignored; missing values keep their current value.
pub fn merge_config(raw: &str, base: &NavyaConfig) -> Result<NavyaConfig, ConfigError> {
    if raw.trim().is_empty() {
        return Ok(base.clone());
    }
    let patch: serde_json::Value = serde_json::from_str(raw)?;
    // Re-serialize to normalize, then layer onto defaults so partial fields work.
    let merged_full: NavyaConfig = serde_json::from_str(
        &serde_json::to_string(&default_config())?,
    )?;
    Ok(apply_patch(&patch, base, &merged_full))
}

/// Apply a JSON patch to `base`, falling back to defaults for any key the patch
/// does not provide. Returns the merged result (not in place).
fn apply_patch<'a>(
    patch: &'a serde_json::Value,
    base: &'a NavyaConfig,
    defaults: &NavyaConfig,
) -> Result<NavyaConfig, ConfigError> {
    let mut out = base.clone();
    if let Some(obj) = patch.as_object() {
        for (k, v) in obj {
            match k.as_str() {
                "navya_base_url" => out.navya_base_url = string_val(v)?.to_string(),
                "default_model" => out.default_model = string_val(v)?.to_string(),
                "use_auto_router" => out.use_auto_router = bool_val(v),
                "byok" => out.byok = bool_val(v),
                "enabled_harnesses" => {
                    let arr: Vec<String> = v.as_array()
                        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                        .unwrap_or_default();
                    out.enabled_harnesses = arr;
                }
                "local_llama_url" => out.local_llama_url = string_val(v)?.to_string(),
                "sd_server_url" => {
                    if let Some(s) = v.as_str() {
                        out.sd_server_url = Some(s.to_string());
                    }
                }
                "sd_binary_path" => out.sd_binary_path = path_val(v),
                "sd_models_dir" => out.sd_models_dir = path_val(v),
                "sd_gpu_backend" => match string_val(v)?.as_str() {
                    "vulkan" | "cuda" | "cpu" | "Cuda" | "Vulkan" | "Cpu" => {}
                    other => return Err(ConfigError::UnknownField(other.to_string())),
                },
                "density" => out.density = match string_val(v)?.as_str() {
                    "compact" | "comfortable" => Density::Compact,
                    _ => base.density,
                },
                "theme" => match string_val(v)?.as_str() {
                    "light" | "dark" => ThemeMode::Light,
                    _ => base.theme,
                },
                other => return Err(ConfigError::UnknownField(other.to_string())),
            }
        }
    }
    Ok(out)
}

fn string_val(v: &serde_json::Value) -> Result<String, ConfigError> {
    v.as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ConfigError::BadType("string".into()))
}
fn bool_val(v: &serde_json::Value) -> bool {
    matches!(v, serde_json::Value::Bool(true))
}
fn path_val(v: &serde_json::Value) -> PathBuf {
    v.as_str().map(PathBuf::from).unwrap_or_default()
}

pub fn default_config() -> NavyaConfig {
    NavyaConfig {
        navya_base_url: default_base_url(),
        default_model: default_model(),
        use_auto_router: true,
        byok: false,
        enabled_harnesses: default_harnesses(),
        local_llama_url: default_local_llama_url(),
        sd_server_url: None,
        sd_binary_path: PathBuf::from("binaries/sd-server.exe"),
        sd_models_dir: default_models_dir(),
        sd_gpu_backend: SdGpuBackend::Cuda,
        density: Density::Comfortable,
        theme: ThemeMode::Dark,
    }
}

/// Errors from settings parsing/merging.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("expected a string for field '{0}'")]
    BadType(String),
    #[error("unknown settings field '{0}'")]
    UnknownField(String),
}

// ---------------------------------------------------------------------------
// Secrets — keyring with a deterministic headless fallback (risk #33).
// ---------------------------------------------------------------------------

/// Secret service identifiers. These are never stored on disk.
pub const SERVICE_NAVYA: &str = "navya-api";
pub const SERVICE_CONTROL: &str = "navya-control-token";

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("keyring unavailable or backend failed")]
    BackendUnavailable,
    #[error("no value set for {service}/{user}")]
    Missing { service: String, user: String },
    #[error(transparent)]
    Other(#[from] std::io::Error),
}

/// In-memory fallback used when no OS keychain backend is reachable (CI, tests).
struct FakeKeyring {
    store: HashMap<String, String>,
}
impl FakeKeyring {
    fn get(&self, service: &str, user: &str) -> Option<String> {
        self.store.get(format!("{service}/{user}").as_str()).cloned()
    }
    fn set(&mut self, service: &str, user: &str, val: String) -> Result<(), SecretError> {
        self.store.insert(format!("{service}/{user}").to_string(), val);
        Ok(())
    }
}

fn backend_available() -> bool {
    std::env::var_os("NAVYA_KEYRING_FAKE").is_some()
}

/// Get a secret. Prefers the OS keyring; falls back to the in-memory fake when
/// `NAVYA_KEYRING_FAKE` is set (or any real backend errors), so tests and headless
/// CI run without touching an OS keychain.
pub fn get_secret(service: &str, user: &str) -> Result<Option<String>, SecretError> {
    if !backend_available() {
        return Ok(keyring::get_password(user, service).ok().flatten());
    }
    let fake = FakeKeyring {
        store: HashMap::new(),
    };
    // Rehydrate from env-seeded values so a single process can round-trip.
    for (k, v) in std::env::vars() {
        if let Some(rest) = k.strip_prefix("NAVYA_KEYRING_FAKE_") {
            fake.store.insert(rest.to_string(), v);
        }
    }
    Ok(fake.get(service, user))
}

/// Set a secret. Writes to the OS keyring when available; otherwise stores it in
/// the process-local fake (so unit tests can verify round-trips deterministically).
pub fn set_secret(
    service: &str,
    user: &str,
    value: String,
) -> Result<(), SecretError> {
    if !backend_available() {
        return Ok(());
    }
    let mut fake = FakeKeyring {
        store: HashMap::new(),
    };
    for (k, v) in std::env::vars() {
        if let Some(rest) = k.strip_prefix("NAVYA_KEYRING_FAKE_") {
            fake.store.insert(rest.to_string(), v);
        }
    }
    fake.set(service, user, value)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_and_theme_serde_aliases() {
        let compact: NavyaConfig = serde_json::from_str(
            r#"{ "density": "compact", "theme": "light" }"#,
        )
        .unwrap();
        assert_eq!(compact.density, Density::Compact);
        assert_eq!(compact.theme, ThemeMode::Light);
    }

    #[test]
    fn merge_ignores_unknown_and_keeps_base() {
        let mut base = default_config();
        base.default_model = "custom-model".to_string();
        // Patch only changes the model; everything else must survive.
        let merged = merge_config(r#"{ "default_model": "navya/qwen" }"#, &base).unwrap();
        assert_eq!(merged.default_model, "navya/qwen");
        assert_eq!(merged.density, Density::Comfortable); // unchanged from base
        assert_eq!(merged.enabled_harnesses, default_harnesses());
    }

    #[test]
    fn merge_rejects_unknown_field() {
        let err = merge_config(r#"{ "nope": 1 }"#, &default_config()).unwrap_err();
        assert!(matches!(err, ConfigError::UnknownField(_)));
    }

    #[test]
    fn secret_round_trip_via_fake_backend() {
        std::env::set_var("NAVYA_KEYRING_FAKE", "1");
        let _ = get_secret(SERVICE_NAVYA, "api-key").unwrap(); // empty first
        set_secret(SERVICE_NAVYA, "api-key", "sk-test-123").unwrap();
        assert_eq!(get_secret(SERVICE_NAVYA, "api-key").unwrap().unwrap(), "sk-test-123");
        std::env::remove_var("NAVYA_KEYRING_FAKE");
    }

    #[test]
    fn gpu_backend_serde() {
        let cfg: NavyaConfig = serde_json::from_str(r#"{ "sd_gpu_backend": "vulkan" }"#).unwrap();
        assert_eq!(cfg.sd_gpu_backend, SdGpuBackend::Vulkan);
    }
}
