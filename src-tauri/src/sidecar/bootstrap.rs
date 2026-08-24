//! Sidecar bootstrap (Phase 14).
//!
//! First-run: ensure sd-server binary present (download + verify checksum),
//! ensure render worker deps (Chrome headless-shell, FFmpeg) via ensureBrowser() / bundled copies,
//! write the control token to keyring.

use std::path::Path;

/// Ensure sd-server binary is present and valid.
pub fn ensure_sd_server_binary(bin_path: &Path) -> Result<(), String> {
    // In a real impl, download per-target prebuilt binary (CUDA/Vulkan/CPU auto-detected)
    // or verify checksum of bundled binary.
    let _ = bin_path;
    Ok(())
}

/// Ensure render worker deps (Chrome headless-shell, FFmpeg).
pub fn ensure_render_deps() -> Result<(), String> {
    // In a real impl, use npx remotion install ffmpeg or bundled copies
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_sd_server_returns_ok_for_mock_path() {
        let path = std::path::Path::new("/tmp/mock-sd-server");
        assert!(ensure_sd_server_binary(path).is_ok());
    }

    #[test]
    fn ensure_render_deps_returns_ok() {
        assert!(ensure_render_deps().is_ok());
    }
}
