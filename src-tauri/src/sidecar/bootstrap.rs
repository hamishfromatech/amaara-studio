//! Sidecar bootstrap (Phase 14).
//!
//! Download-on-first-run for the `sd-server` binary (per NOTES.md): if the
//! configured binary is missing, fetch it from `AMAARA_SD_RELEASE_BASE`,
//! verify its SHA256, and write atomically. Progress/failures surface through
//! the `on_progress` callback so command callers can route them into
//! `SidecarEvent::LogLine` (status-strip log drawer) — the module itself
//! stays UI-free and testable.
//!
//! Checksum contract: `scripts/build-sidecars.*` stages
//! `sd-server-<triple>[.exe]` plus a `<file>.sha256` sidecar containing
//! `<hex>  <filename>`. When a binary is already present, its sidecar (if
//! any) is verified before use.
//!
//! Allow(dead_code): the sd-server spawn path (Phase 8 finish) is not wired
//! yet; callers arrive when local image generation goes live (same as
//! `sd/mod.rs`).
#![allow(dead_code)]

use std::path::Path;

use sha2::{Digest, Sha256};

/// Hex-encoded SHA256 of a byte slice.
pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Hex-encoded SHA256 of a file's contents.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(hex_sha256(&bytes))
}

/// True when `bytes` hashes to `expected_hex` (case-insensitive).
pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> bool {
    hex_sha256(bytes) == expected_hex.trim().to_lowercase()
}

/// `<file>.sha256` path for a binary path.
fn sidecar_path(bin_path: &Path) -> std::path::PathBuf {
    let mut s = bin_path.as_os_str().to_os_string();
    s.push(".sha256");
    std::path::PathBuf::from(s)
}

/// Verify an already-present binary against its `<file>.sha256` sidecar.
/// Missing sidecar → Ok (unverified but present).
fn verify_sidecar_checksum(bin_path: &Path) -> Result<(), String> {
    let sidecar = sidecar_path(bin_path);
    if !sidecar.exists() {
        return Ok(());
    }
    let expected = std::fs::read_to_string(&sidecar)
        .map_err(|e| format!("cannot read {}: {e}", sidecar.display()))?;
    let expected_hex = expected.split_whitespace().next().unwrap_or("").to_string();
    let actual = sha256_file(bin_path)?;
    if actual != expected_hex {
        return Err(format!(
            "sd-server binary at {} does not match its checksum (expected {expected_hex}, got {actual}). Re-run scripts/build-sidecars or delete the binary to re-download.",
            bin_path.display()
        ));
    }
    Ok(())
}

/// Best-effort host target triple (for release asset naming).
pub fn host_triple() -> String {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };
    match std::env::consts::OS {
        "windows" => format!("{arch}-pc-windows-msvc"),
        "macos" => format!("{arch}-apple-darwin"),
        "linux" => format!("{arch}-unknown-linux-gnu"),
        other => format!("{arch}-unknown-{other}"),
    }
}

/// Download `url` to `dest` atomically (temp + rename), verifying the SHA256
/// when `expected_hex` is given. Progress lines go to `on_progress`.
pub async fn download_to(
    dest: &Path,
    url: &str,
    expected_hex: Option<&str>,
    on_progress: &mut (dyn FnMut(&str) + Send),
) -> Result<(), String> {
    on_progress(&format!("downloading {url}"));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download failed: HTTP {}", resp.status()));
    }
    let total = resp.content_length();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("download failed: {e}"))?;
    match total {
        Some(t) => on_progress(&format!("downloaded {} / {t} bytes", bytes.len())),
        None => on_progress(&format!("downloaded {} bytes", bytes.len())),
    }

    if let Some(expected) = expected_hex {
        if !verify_sha256(&bytes, expected) {
            return Err(format!("checksum mismatch for {url} (expected {expected})"));
        }
        on_progress("checksum ok");
    }

    // Atomic write: temp file in the same dir, then rename over the dest.
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    let tmp = parent.join(format!(
        ".{}.part",
        dest.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("sidecar")
    ));
    std::fs::write(&tmp, &bytes).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, dest).map_err(|e| format!("cannot finalize {}: {e}", dest.display()))?;
    Ok(())
}

/// Ensure the sd-server binary is present and checksum-verified.
///
/// Order: existing binary (verify `.sha256` sidecar if present) →
/// download-on-first-run from `AMAARA_SD_RELEASE_BASE` at
/// `{base}/{flavor}/sd-server-{triple}[.exe]` (checksum taken from
/// `{base}/.../sd-server-{triple}[.exe].sha256` when available).
///
/// `flavor` is one of `cpu` | `cuda` | `vulkan` (config `sd_backend`).
pub async fn ensure_sd_server_binary(
    bin_path: &Path,
    flavor: &str,
    on_progress: &mut (dyn FnMut(&str) + Send),
) -> Result<(), String> {
    if bin_path.exists() {
        on_progress(&format!("sd-server found at {}", bin_path.display()));
        return verify_sidecar_checksum(bin_path);
    }

    let base = std::env::var("AMAARA_SD_RELEASE_BASE").unwrap_or_default();
    if base.trim().is_empty() {
        return Err(
            "sd-server binary not found for local image generation. Point Settings → sd_binary_path at an installed sd-server, or set AMAARA_SD_RELEASE_BASE to enable download-on-first-run."
                .to_string(),
        );
    }
    let triple = host_triple();
    let ext = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    let stem = format!("sd-server-{triple}{ext}");
    let url = format!("{}/{flavor}/{stem}", base.trim_end_matches('/'));

    download_to(bin_path, &url, None, on_progress).await?;
    // Prefer a published checksum sidecar next to the binary asset.
    let sidecar_url = format!("{url}.sha256");
    match reqwest::get(&sidecar_url).await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(text) = resp.text().await {
                let expected = text.split_whitespace().next().unwrap_or("").to_string();
                if !expected.is_empty() {
                    let bytes = std::fs::read(bin_path).map_err(|e| format!("read back: {e}"))?;
                    if !verify_sha256(&bytes, &expected) {
                        let _ = std::fs::remove_file(bin_path);
                        return Err(format!(
                            "downloaded sd-server failed checksum verification (expected {expected}); removed the bad download"
                        ));
                    }
                    // Persist the sidecar next to the binary for future runs.
                    let _ = std::fs::write(sidecar_path(bin_path), text);
                    on_progress("checksum ok");
                }
            }
        }
        _ => on_progress("no published checksum; skipping verification"),
    }
    on_progress(&format!("sd-server ready at {}", bin_path.display()));
    Ok(())
}

/// Ensure render-worker deps exist: the worker script is present and Node is
/// resolvable (the worker + `npx hyperframes render` both need it).
pub fn ensure_render_deps(worker_path: &Path) -> Result<(), String> {
    if !worker_path.exists() {
        return Err(format!(
            "render worker not found at {}. Reinstall the app or check the bundle layout.",
            worker_path.display()
        ));
    }
    if !node_on_path() {
        return Err(
            "Node.js (≥ 22) is required for rendering but was not found on PATH.".to_string(),
        );
    }
    Ok(())
}

fn lookup_bin() -> &'static str {
    if cfg!(windows) {
        "where"
    } else {
        "which"
    }
}

fn node_on_path() -> bool {
    std::process::Command::new(lookup_bin())
        .arg("node")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_digest() {
        // sha256("hello") — well-known digest.
        assert_eq!(
            hex_sha256(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn verify_sha256_is_case_insensitive_and_rejects_wrong() {
        assert!(verify_sha256(
            b"hello",
            "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824"
        ));
        assert!(!verify_sha256(b"hello", "deadbeef"));
    }

    #[test]
    fn sha256_file_reads_disk() {
        let tmp = std::env::temp_dir().join(format!("amaara-bootstrap-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let f = tmp.join("blob.bin");
        std::fs::write(&f, b"hello").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn existing_binary_with_bad_sidecar_fails() {
        let tmp =
            std::env::temp_dir().join(format!("amaara-bootstrap-side-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let bin = tmp.join("sd-server.exe");
        std::fs::write(&bin, b"payload").unwrap();
        std::fs::write(
            sidecar_path(&bin),
            "0000000000000000000000000000000000000000000000000000000000000000  sd-server.exe",
        )
        .unwrap();

        let mut progress: Vec<String> = Vec::new();
        let err = ensure_sd_server_binary(&bin, "cpu", &mut |m: &str| progress.push(m.into()))
            .await
            .unwrap_err();
        assert!(err.contains("does not match"), "err: {err}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn existing_binary_with_good_sidecar_passes() {
        let tmp =
            std::env::temp_dir().join(format!("amaara-bootstrap-side-ok-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let bin = tmp.join("sd-server.exe");
        std::fs::write(&bin, b"payload").unwrap();
        let digest = hex_sha256(b"payload");
        std::fs::write(sidecar_path(&bin), format!("{digest}  sd-server.exe")).unwrap();

        let mut progress: Vec<String> = Vec::new();
        ensure_sd_server_binary(&bin, "cpu", &mut |m: &str| progress.push(m.into()))
            .await
            .expect("good checksum should pass");
        assert!(progress.iter().any(|m| m.contains("found at")));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn missing_binary_without_release_base_errors_with_action() {
        let tmp =
            std::env::temp_dir().join(format!("amaara-bootstrap-miss-{}", std::process::id()));
        let bin = tmp.join("missing").join("sd-server.exe");
        std::env::remove_var("AMAARA_SD_RELEASE_BASE");
        let mut progress: Vec<String> = Vec::new();
        let err = ensure_sd_server_binary(&bin, "cpu", &mut |m: &str| progress.push(m.into()))
            .await
            .unwrap_err();
        assert!(err.contains("sd_binary_path") && err.contains("AMAARA_SD_RELEASE_BASE"));
    }

    #[test]
    fn render_deps_fail_on_missing_worker() {
        let err = ensure_render_deps(Path::new("definitely/not/here.mjs")).unwrap_err();
        assert!(err.contains("render worker not found"));
    }

    #[test]
    fn render_deps_ok_with_worker_and_node() {
        // Pass path only when node is resolvable in this environment.
        let worker =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries/render-worker.mjs");
        if node_on_path() {
            assert!(ensure_render_deps(&worker).is_ok());
        }
    }
}
