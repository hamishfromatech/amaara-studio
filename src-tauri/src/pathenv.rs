//! PATH reconstruction for GUI-launched processes.
//!
//! A Finder/Dock launch on macOS gets the minimal launchd PATH
//! (`/usr/bin:/bin:/usr/sbin:/sbin`) — the user's installed CLIs
//! (a-coder-cli, node, ffmpeg, …) are invisible to `which` and to every
//! spawned child. The a-coder-cli desktop app solves the same problem by
//! reconstructing the user's PATH: process PATH + login-shell PATH + common
//! install locations + nvm version dirs, deduplicated. Applied once at app
//! startup so every consumer (harness discovery, sidecars, render deps,
//! spawned children) inherits a usable PATH.

use std::collections::HashSet;

fn path_separator() -> char {
    if cfg!(windows) {
        ';'
    } else {
        ':'
    }
}

fn expand_home(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = if cfg!(windows) {
            std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .ok()
        } else {
            std::env::var("HOME").ok()
        };
        if let Some(home) = home {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

/// The user's shell PATH via `$SHELL -c 'echo $PATH'`. Non-interactive shells
/// skip the interactive rc files, so this may return a short PATH — the
/// common-dirs list below covers the rest.
#[cfg(unix)]
fn user_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let output = std::process::Command::new(&shell)
        .arg("-c")
        .arg("echo $PATH")
        .output()
        .ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(path);
        }
    }
    None
}

/// Build the reconstructed PATH: process PATH first (preserves any explicit
/// override), then the user's shell PATH, then common tool install locations,
/// then every nvm node version. Deduplicated, order preserved.
pub fn reconstructed_path() -> String {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(path_var) = std::env::var("PATH") {
        let sep = path_separator();
        for part in path_var.split(sep) {
            dirs.push(std::path::PathBuf::from(part));
        }
    }

    #[cfg(unix)]
    if let Some(shell_path) = user_shell_path() {
        for part in shell_path.split(':') {
            dirs.push(std::path::PathBuf::from(part));
        }
    }

    let common = [
        "/usr/local/bin",
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "~/.a-coder/bin",
        "~/.a-coder/cli/bin",
        "~/.npm-global/bin",
        "~/.local/bin",
        "~/.yarn/bin",
        "/usr/local/lib/node_modules/.bin",
        "/opt/homebrew/lib/node_modules/.bin",
    ];
    for raw in &common {
        dirs.push(expand_home(raw));
    }

    // nvm installs: one bin dir per installed node version.
    if let Ok(home) = std::env::var("HOME") {
        let nvm_versions = std::path::PathBuf::from(&home).join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(&nvm_versions) {
            for entry in entries.flatten() {
                let bin = entry.path().join("bin");
                if bin.is_dir() {
                    dirs.push(bin);
                }
            }
        }
    }

    let mut seen = HashSet::new();
    dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));

    let sep = path_separator();
    dirs.iter()
        .map(|p| p.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(&sep.to_string())
}

/// Replace the process PATH with the reconstructed one. Call once at app
/// startup, before any binary discovery or child spawn. Idempotent enough:
/// the reconstruction starts from the current PATH, so re-applying only
/// appends.
pub fn apply_reconstructed_path() {
    let before = std::env::var("PATH").unwrap_or_default();
    let reconstructed = reconstructed_path();
    if reconstructed != before {
        std::env::set_var("PATH", &reconstructed);
        tracing::info!(
            "PATH reconstructed for GUI launch ({} entries → {} entries)",
            before.split(path_separator()).count(),
            reconstructed.split(path_separator()).count()
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconstructed_path_includes_system_and_common_dirs() {
        let p = reconstructed_path();
        assert!(p.contains("/usr/bin"), "system dir missing: {p}");
        assert!(!p.starts_with(path_separator()), "leading separator: {p}");
        assert!(!p.ends_with(path_separator()), "trailing separator: {p}");
        // No duplicate entries.
        let parts: Vec<&str> = p.split(path_separator()).collect();
        let unique: std::collections::HashSet<&&str> = parts.iter().collect();
        assert_eq!(parts.len(), unique.len(), "duplicates in {p}");
    }
}
