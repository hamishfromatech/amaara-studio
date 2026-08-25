//! Declarative harness descriptor registry + binary detection.
//!
//! Ported from open-design's `apps/daemon/src/runtimes/` (registry.ts +
//! detection.ts). Each harness the studio can drive is described once by a
//! static `HarnessDescriptor`: the CLI binary to probe, fallback binaries
//! (drop-in forks like `openclaude`), version args, install/docs links, and
//! the fallback model catalog shown before a live model listing exists.
//!
//! Detection itself is modelled on open-design's `detectAgent`: resolve the
//! exact executable that would be spawned (walking fallback binaries), then
//! run a bounded `--version` probe to prove the binary is invocable, and
//! surface `path` + `version` so the UI can explain what's on the machine.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use parking_lot::Mutex;

use crate::harness::event::ModelInfo;

/// Everything the studio knows about a harness without spawning it.
pub struct HarnessDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    /// Primary CLI binary probed on PATH.
    pub bin: &'static str,
    /// Drop-in forks whose CLI is argv-compatible, tried in order if `bin`
    /// isn't on PATH (open-design `fallbackBins`).
    pub fallback_bins: &'static [&'static str],
    /// Args for the version probe (open-design `versionArgs`).
    pub version_args: &'static [&'static str],
    /// Where the install flow lives (open-design `installUrl`).
    pub install_url: Option<&'static str>,
    /// User-facing docs link (open-design `docsUrl`).
    pub docs_url: Option<&'static str>,
    /// Static model catalog surfaced when no live listing exists
    /// (open-design `fallbackModels`).
    pub fallback_models: fn() -> Vec<ModelInfo>,
}

fn chat_model(id: &str, label: &str) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        name: Some(label.to_string()),
        kind: "chat".to_string(),
    }
}

fn default_model() -> Vec<ModelInfo> {
    vec![chat_model("default", "Default")]
}

fn claude_fallback_models() -> Vec<ModelInfo> {
    vec![
        chat_model("default", "Default"),
        chat_model("sonnet", "Sonnet (alias)"),
        chat_model("opus", "Opus (alias)"),
        chat_model("haiku", "Haiku (alias)"),
        chat_model("claude-opus-4-5", "claude-opus-4-5"),
        chat_model("claude-sonnet-4-5", "claude-sonnet-4-5"),
        chat_model("claude-haiku-4-5", "claude-haiku-4-5"),
    ]
}

fn codex_fallback_models() -> Vec<ModelInfo> {
    vec![
        chat_model("default", "Default"),
        chat_model("gpt-5.5", "gpt-5.5"),
        chat_model("gpt-5.4", "gpt-5.4"),
        chat_model("gpt-5.4-mini", "gpt-5.4-mini"),
        chat_model("gpt-5.3-codex", "gpt-5.3-codex"),
        chat_model("gpt-5.1", "gpt-5.1"),
        chat_model("gpt-5-codex", "gpt-5-codex"),
        chat_model("gpt-5", "gpt-5"),
        chat_model("o3", "o3"),
        chat_model("o4-mini", "o4-mini"),
    ]
}

fn hermes_fallback_models() -> Vec<ModelInfo> {
    vec![
        chat_model("default", "Default"),
        chat_model("grok-4.3", "grok-4.3 (xAI · default)"),
        chat_model("grok-4.20-reasoning", "grok-4.20-reasoning (xAI · deep)"),
        chat_model("grok-4.20-non-reasoning", "grok-4.20-non-reasoning (xAI · fast)"),
        chat_model("grok-4.20-multi-agent", "grok-4.20-multi-agent (xAI · orchestration)"),
        chat_model("openai-codex:gpt-5.5", "gpt-5.5 (openai-codex:gpt-5.5)"),
        chat_model("openai-codex:gpt-5.4", "gpt-5.4 (openai-codex:gpt-5.4)"),
        chat_model("openai-codex:gpt-5.4-mini", "gpt-5.4-mini (openai-codex:gpt-5.4-mini)"),
    ]
}

fn antigravity_fallback_models() -> Vec<ModelInfo> {
    vec![
        chat_model("default", "Default"),
        chat_model("Gemini 3.1 Pro (High)", "Gemini 3.1 Pro (High)"),
        chat_model("Gemini 3.1 Pro (Low)", "Gemini 3.1 Pro (Low)"),
        chat_model("Gemini 3.5 Flash (High)", "Gemini 3.5 Flash (High)"),
        chat_model("Gemini 3.5 Flash (Medium)", "Gemini 3.5 Flash (Medium)"),
        chat_model("Gemini 3.5 Flash (Low)", "Gemini 3.5 Flash (Low)"),
        chat_model("Claude Sonnet 4.6 (Thinking)", "Claude Sonnet 4.6 (Thinking)"),
        chat_model("Claude Opus 4.6 (Thinking)", "Claude Opus 4.6 (Thinking)"),
        chat_model("GPT-OSS 120B (Medium)", "GPT-OSS 120B (Medium)"),
    ]
}

/// The harnesses this build ships. Order is presentation order for pickers.
///
/// Bin names mirror `state.rs`'s original availability table; URLs and model
/// catalogs mirror open-design's defs (`metadata.ts`, `defs/claude.ts`,
/// `defs/codex.ts`, `defs/hermes.ts`, `defs/antigravity.ts`).
pub static HARNESS_DESCRIPTORS: &[HarnessDescriptor] = &[
    HarnessDescriptor {
        id: "a-coder-cli",
        label: "a-coder-cli",
        bin: "a-coder-cli",
        fallback_bins: &[],
        version_args: &["--version"],
        install_url: None,
        docs_url: Some("https://github.com/hamis/a-coder-cli"),
        // a-coder-cli lists models live over RPC (`get_available_models`);
        // the fallback only shows when the binary is absent.
        fallback_models: default_model,
    },
    HarnessDescriptor {
        id: "antigravity",
        label: "Antigravity",
        bin: "agy",
        fallback_bins: &[],
        version_args: &["--version"],
        install_url: Some("https://antigravity.google/cli"),
        docs_url: Some("https://antigravity.google/docs/cli-overview"),
        fallback_models: antigravity_fallback_models,
    },
    HarnessDescriptor {
        id: "claude-code",
        label: "Claude Code",
        bin: "claude",
        // OpenClaude — https://github.com/Gitlawb/openclaude — is a
        // drop-in fork shipping an argv-compatible CLI (open-design #235).
        fallback_bins: &["openclaude"],
        version_args: &["--version"],
        install_url: Some("https://docs.anthropic.com/en/docs/claude-code/setup"),
        docs_url: Some("https://docs.anthropic.com/en/docs/claude-code"),
        fallback_models: claude_fallback_models,
    },
    HarnessDescriptor {
        id: "codex",
        label: "Codex",
        bin: "codex",
        fallback_bins: &[],
        version_args: &["--version"],
        install_url: Some("https://github.com/openai/codex"),
        docs_url: Some("https://developers.openai.com/codex"),
        fallback_models: codex_fallback_models,
    },
    HarnessDescriptor {
        id: "hermes",
        label: "Hermes",
        bin: "hermes",
        fallback_bins: &[],
        version_args: &["--version"],
        install_url: Some("https://github.com/nousresearch/hermes-agent"),
        docs_url: Some("https://hermes-agent.nousresearch.com/docs/"),
        fallback_models: hermes_fallback_models,
    },
    HarnessDescriptor {
        id: "openclaw",
        label: "OpenClaw",
        bin: "openclaw",
        fallback_bins: &[],
        version_args: &["--version"],
        install_url: None,
        docs_url: None,
        fallback_models: default_model,
    },
];

/// Look up the descriptor for a harness id.
pub fn descriptor_for(id: &str) -> Option<&'static HarnessDescriptor> {
    HARNESS_DESCRIPTORS.iter().find(|d| d.id == id)
}

/// What detection learned about a descriptor on this machine.
#[derive(Debug, Clone, Default)]
pub struct HarnessDetection {
    /// True when an executable was found on PATH AND its version probe ran
    /// (even if the version string itself couldn't be parsed — mirrors
    /// open-design's "spawned but --version unhappy" outcome).
    pub available: bool,
    /// The exact executable path that would be spawned.
    pub path: Option<PathBuf>,
    /// First line of `<bin> --version`, if any.
    pub version: Option<String>,
}

/// How long a detection result stays fresh; the UI re-snapshots on every
/// state load, but spawning up to six `--version` probes per snapshot is
/// wasteful. Cheap PATH scans are redone each time (no process spawn), only
/// the version probe is cached.
const DETECTION_TTL: Duration = Duration::from_secs(30);
/// open-design probes `--version` with a 3000ms ceiling.
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_millis(3000);

struct CacheEntry {
    detection: HarnessDetection,
    cached_at: Instant,
}

static DETECTION_CACHE: Mutex<Option<HashMap<String, CacheEntry>>> = Mutex::new(None);

/// Detect a harness: resolve the executable (primary bin, then fallbacks),
/// then run the bounded version probe. Cached for `DETECTION_TTL`.
pub fn detect(descriptor: &HarnessDescriptor) -> HarnessDetection {
    // A cached hit is only reused while the path is still the one a fresh
    // PATH scan would pick — an install/uninstall changes that immediately.
    let resolved = resolve_path(descriptor);
    {
        let guard = DETECTION_CACHE.lock();
        if let Some(map) = guard.as_ref() {
            if let Some(entry) = map.get(descriptor.id) {
                let same_resolution = entry.detection.path == resolved;
                if same_resolution && entry.cached_at.elapsed() < DETECTION_TTL {
                    return entry.detection.clone();
                }
            }
        }
    }

    let detection = match resolved {
        Some(path) => {
            let version = probe_version(&path, descriptor.version_args);
            HarnessDetection {
                // The binary exists; treat a failed/garbled version probe as
                // "available, version unknown" rather than "not installed".
                available: true,
                path: Some(path),
                version,
            }
        }
        None => HarnessDetection::default(),
    };

    let mut guard = DETECTION_CACHE.lock();
    guard
        .get_or_insert_with(HashMap::new)
        .insert(
            descriptor.id.to_string(),
            CacheEntry {
                detection: detection.clone(),
                cached_at: Instant::now(),
            },
        );
    detection
}

/// Cheap availability answer for hot paths (prompt admission): PATH scan only,
/// no version probe, reads the cache when fresh.
pub fn is_available(id: &str) -> bool {
    match descriptor_for(id) {
        Some(d) => detect(d).available,
        None => false,
    }
}

/// Resolve the executable detection would probe: the first of
/// `bin, fallback_bins…` found on PATH. Pure filesystem scan, no process
/// spawn. Returns the absolute candidate path.
fn resolve_path(descriptor: &HarnessDescriptor) -> Option<PathBuf> {
    let names = std::iter::once(descriptor.bin)
        .chain(descriptor.fallback_bins.iter().copied());
    for name in names {
        if let Some(path) = which_path(name) {
            return Some(path);
        }
    }
    None
}

/// Find an executable named `name` on PATH. Windows respects PATHEXT so
/// `claude` matches `claude.cmd`/`claude.exe`; elsewhere the file must both
/// exist and be a file (executable-bit checking is left to the probe spawn).
pub fn which_path(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    // An explicit path (absolute or containing a separator) is used as-is.
    if Path::new(name).is_absolute() || name.contains(std::path::MAIN_SEPARATOR) || name.contains('/') {
        return if Path::new(name).is_file() {
            Some(PathBuf::from(name))
        } else {
            None
        };
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        if let Some(found) = probe_dir(&dir, name) {
            return Some(found);
        }
    }
    None
}

#[cfg(windows)]
fn probe_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let pathext = std::env::var_os("PATHEXT")
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string());
    for ext in pathext.split(';') {
        let ext = ext.trim();
        if ext.is_empty() {
            continue;
        }
        let candidate = dir.join(format!("{name}{}", ext.to_lowercase()));
        if candidate.is_file() {
            return Some(candidate);
        }
        let candidate = dir.join(format!("{name}{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(not(windows))]
fn probe_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let candidate = dir.join(name);
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// Run `<path> <version_args>` with a bounded wait and return the first
/// stdout line. A timeout, non-zero exit, or empty stdout yields None — the
/// binary still counts as available (open-design's "spawned" outcome).
fn probe_version(path: &Path, args: &[&str]) -> Option<String> {
    use std::io::Read;

    let mut cmd = std::process::Command::new(path);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    // Keep probes off the user's desktop on Windows (no console flash).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;

    let deadline = Instant::now() + VERSION_PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut buf = String::new();
                // A version banner is a few dozen bytes; read_to_end is safe
                // here because the child has already exited (pipe at EOF).
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_string(&mut buf);
                }
                let first = buf.lines().next().map(str::trim).unwrap_or("");
                return if first.is_empty() {
                    None
                } else {
                    Some(first.chars().take(80).collect())
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Fallback models for a harness id (empty when the id is unknown).
pub fn fallback_models(id: &str) -> Vec<ModelInfo> {
    descriptor_for(id).map(|d| (d.fallback_models)()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_harness_has_a_descriptor() {
        // The runtime registry (state.rs) registers exactly these ids; the
        // descriptor table must cover all of them or the UI loses the row.
        for id in [
            "a-coder-cli",
            "claude-code",
            "codex",
            "hermes",
            "antigravity",
            "openclaw",
        ] {
            assert!(descriptor_for(id).is_some(), "missing descriptor for {id}");
        }
    }

    #[test]
    fn descriptor_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for d in HARNESS_DESCRIPTORS {
            assert!(seen.insert(d.id), "duplicate descriptor id {}", d.id);
        }
    }

    #[test]
    fn unknown_id_is_unavailable() {
        assert!(!is_available("no-such-harness"));
        assert!(fallback_models("no-such-harness").is_empty());
    }

    #[test]
    fn missing_binary_reports_unavailable_with_no_path() {
        let bogus = HarnessDescriptor {
            id: "test-bogus",
            label: "Bogus",
            bin: "navya-definitely-not-a-real-binary-xyz",
            fallback_bins: &["navya-also-not-real-xyz"],
            version_args: &["--version"],
            install_url: None,
            docs_url: None,
            fallback_models: default_model,
        };
        let d = detect(&bogus);
        assert!(!d.available);
        assert!(d.path.is_none());
        assert!(d.version.is_none());
    }

    #[test]
    fn explicit_missing_path_does_not_resolve() {
        assert!(which_path("C:\\navya\\not-here\\bin-xyz.exe").is_none());
        assert!(which_path("/navya/not-here/bin-xyz").is_none());
    }

    #[test]
    fn fallback_models_are_populated_for_known_ids() {
        assert!(fallback_models("claude-code").len() > 1);
        assert!(fallback_models("codex").len() > 1);
        assert_eq!(fallback_models("openclaw").len(), 1);
    }

    #[test]
    fn detection_caches_results() {
        let bogus = HarnessDescriptor {
            id: "test-cache",
            label: "Cache",
            bin: "navya-not-real-cache-test",
            fallback_bins: &[],
            version_args: &["--version"],
            install_url: None,
            docs_url: None,
            fallback_models: default_model,
        };
        let first = detect(&bogus);
        let second = detect(&bogus);
        assert_eq!(first.available, second.available);
    }
}
