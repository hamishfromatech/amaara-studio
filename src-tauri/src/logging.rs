//! Structured logging (Phase 15).
//!
//! Installs a global `tracing` subscriber that writes one JSON line per event
//! to a daily-rotating file under the app-data `logs/` dir
//! (`navya.<YYYY-MM-DD>.log`). JSON keeps the logs machine-parseable for the
//! "send feedback" packager (Phase 15) and the per-sidecar log drawer.
//!
//! The install is best-effort: if the log dir can't be created (headless,
//! tests, restricted profiles) it falls back to a stderr subscriber so output
//! is never silently dropped. Callers should treat a failed file install as
//! non-fatal.

use std::path::Path;

use tracing_appender::rolling;
use tracing_subscriber::EnvFilter;

/// Default level filter when `RUST_LOG` is unset. `navya_studio=debug` gives the
/// app itself slightly more verbosity than the (loud) third-party crates.
const DEFAULT_FILTER: &str = "info,navya_studio=debug";

/// Install the global tracing subscriber writing daily-rotating JSON logs to
/// `<log_dir>/navya.<YYYY-MM-DD>.log`. Best-effort: falls back to stderr if the
/// dir can't be created or a subscriber is already installed.
pub fn init(log_dir: &Path) {
    if install_file(log_dir).is_err() {
        let _ = install_stderr();
    }
}

fn install_file(log_dir: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    std::fs::create_dir_all(log_dir)?;
    // `rolling::daily` rotates once per day (prefix.YYYY-MM-DD).
    let appender = rolling::daily(log_dir, "navya");
    let filter = env_filter();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(appender)
        .json()
        .try_init()?;
    Ok(())
}

fn install_stderr() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = env_filter();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init()?;
    Ok(())
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_falls_back_to_stderr_when_dir_unwritable() {
        // An unwritable path (a file, not a dir) forces the stderr fallback.
        let tmp = std::env::temp_dir().join(format!("navya-log-bad-{}", std::process::id()));
        std::fs::write(&tmp, b"x").ok();
        // Must not panic even though the file install fails.
        init(&tmp);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn init_writes_json_to_log_dir() {
        let dir = std::env::temp_dir().join(format!("navya-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        init(&dir);
        // Emit an event and confirm a JSON log line lands in the dir.
        tracing::info!("phase15 smoke test");
        // Allow the rolling writer a moment to flush.
        std::thread::sleep(Duration::from_millis(100));
        let files: Vec<_> = std::fs::read_dir(&dir)
            .expect("log dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        assert!(
            !files.is_empty(),
            "expected at least one rotated log file in {dir:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
