//! "Send feedback" packager (Phase 15).
//!
//! Bundles the last few days of rotating JSON logs plus a redacted session
//! summary into a single zip the user can attach to a bug report. Privacy:
//! every bundled line is redacted — the home directory and the app-data dir
//! are replaced with `~` (raw, JSON-escaped, and forward-slash forms), so
//! absolute paths and usernames never leave the machine.

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// How many most-recent log files to include.
const MAX_LOG_FILES: usize = 3;

/// Replace known local paths in `text` with `~`, covering the raw form
/// (plain text), the JSON-escaped form (`\` → `\\`, seen inside JSON log
/// lines on Windows), and the forward-slash form (Node/FFmpeg output).
pub fn redact(text: &str, roots: &[PathBuf]) -> String {
    let mut out = text.to_string();
    for root in roots {
        let raw = root.to_string_lossy().to_string();
        out = out.replace(&raw, "~");
        out = out.replace(&raw.replace('\\', "\\\\"), "~");
        out = out.replace(&raw.replace('\\', "/"), "~");
    }
    out
}

/// Home directory best-effort (tests pass explicit roots instead).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// Newest-first list of log files under `<data_dir>/logs/`.
fn recent_logs(log_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return vec![];
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("navya.") && n.ends_with(".log"))
                .unwrap_or(false)
        })
        .filter_map(|p| {
            let modified = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            Some((modified, p))
        })
        .collect();
    files.sort_by_key(|(m, _)| std::cmp::Reverse(*m));
    files.into_iter().take(MAX_LOG_FILES).map(|(_, p)| p).collect()
}

/// Build `navya-feedback-<timestamp>.zip` in `out_dir` containing the
/// redacted recent logs and an optional session summary. Returns the zip path.
pub fn package(
    data_dir: &Path,
    out_dir: &Path,
    session_summary: Option<&serde_json::Value>,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("cannot create feedback dir {}: {e}", out_dir.display()))?;

    // Redaction roots: app-data dir first (most specific), then home, so the
    // app-data path inside home is fully collapsed before home is applied.
    let mut roots: Vec<PathBuf> = vec![data_dir.to_path_buf()];
    if let Some(home) = home_dir() {
        if !roots.contains(&home) {
            roots.push(home);
        }
    }

    let zip_path = out_dir.join(format!("navya-feedback-{}.zip", chrono_stamp()));
    let file = std::fs::File::create(&zip_path)
        .map_err(|e| format!("cannot create {}: {e}", zip_path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Redacted session summary (config has no secrets — they live in the
    // keyring, enforced by the leak test in config.rs).
    if let Some(summary) = session_summary {
        let redacted = redact(&serde_json::to_string_pretty(summary).unwrap_or_default(), &roots);
        zip.start_file("session.json", options)
            .map_err(|e| format!("zip write failed: {e}"))?;
        let _ = zip.write_all(redacted.as_bytes());
    }

    for log in recent_logs(&data_dir.join("logs")) {
        let name = log
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("navya.log")
            .to_string();
        let body = std::fs::read_to_string(&log).unwrap_or_default();
        let redacted = redact(&body, &roots);
        zip.start_file(format!("logs/{name}"), options)
            .map_err(|e| format!("zip write failed: {e}"))?;
        let _ = zip.write_all(redacted.as_bytes());
    }

    zip.finish()
        .map_err(|e| format!("zip finalize failed: {e}"))?;
    Ok(zip_path)
}

/// `YYYYMMDD-HHMMSS` UTC timestamp for feedback filenames (no chrono dep).
fn chrono_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    if mth <= 2 {
        y += 1;
    }
    format!("{y:04}{mth:02}{d:02}-{h:02}{m:02}{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_strips_raw_and_escaped_paths() {
        let data = PathBuf::from(r"C:\Users\me\AppData\Roaming\navya");
        let home = PathBuf::from(r"C:\Users\me");
        let roots = vec![data.clone(), home];

        // Raw path (Windows separator preserved after the ~).
        assert_eq!(
            redact(r"opened C:\Users\me\AppData\Roaming\navya\navya.db", &roots),
            r"opened ~\navya.db"
        );
        // JSON-escaped form inside a log line.
        let line = r#"{\"path\":\"C:\\Users\\me\\AppData\\Roaming\\navya\\x\"}"#;
        assert_eq!(redact(line, &roots), r#"{\"path\":\"~\\x\"}"#);
        // Forward-slash form.
        assert_eq!(
            redact("at C:/Users/me/AppData/Roaming/navya/out.mp4", &roots),
            "at ~/out.mp4"
        );
        // Home-only path (app-data replaced first, then home).
        assert_eq!(
            redact(r"C:\Users\me\Documents\f.txt", &roots),
            r"~\Documents\f.txt"
        );
    }

    #[test]
    fn redact_leaves_unknown_text_untouched() {
        let roots = vec![PathBuf::from(r"C:\nope")];
        assert_eq!(redact("nothing to see here", &roots), "nothing to see here");
    }

    #[test]
    fn package_writes_zip_with_redacted_logs() {
        let tmp = std::env::temp_dir().join(format!("navya-feedback-test-{}", std::process::id()));
        let data_dir = tmp.join("data");
        let logs = data_dir.join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        // JSON-escaped path under the real data_dir (as tracing JSON would
        // emit it) so the packager's redaction roots actually match.
        let fake_db = data_dir.join("navya.db").to_string_lossy().replace('\\', "\\\\");
        std::fs::write(
            logs.join("navya.2026-08-27.log"),
            format!(r#"{{"message":"loaded {fake_db}"}}"#),
        )
        .unwrap();
        let out_dir = tmp.join("out");
        // Use a path under the real data_dir so the packager's redaction roots
        // (data_dir + home) actually match.
        let summary = serde_json::json!({ "project": "demo", "dir": data_dir.join("projects").join("demo").to_string_lossy() });

        let zip_path = package(&data_dir, &out_dir, Some(&summary)).expect("package");
        assert!(zip_path.exists());
        assert!(
            zip_path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("navya-feedback-")
        );

        let f = std::fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(f).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "session.json"), "names: {names:?}");
        assert!(
            names.iter().any(|n| n.starts_with("logs/navya.")),
            "names: {names:?}"
        );

        // The session summary must not leak the home path.
        {
            let mut session = archive.by_name("session.json").unwrap();
            let mut body = String::new();
            std::io::Read::read_to_string(&mut session, &mut body).unwrap();
            assert!(!body.contains("AppData"), "session leaked a path: {body}");
            assert!(body.contains('~'));
        }

        // The log line must not leak the home path either.
        let log_name = names
            .iter()
            .find(|n| n.starts_with("logs/navya."))
            .unwrap()
            .clone();
        let mut logf = archive.by_name(&log_name).unwrap();
        let mut lbody = String::new();
        std::io::Read::read_to_string(&mut logf, &mut lbody).unwrap();
        assert!(!lbody.contains("AppData"), "log leaked a path: {lbody}");
        assert!(lbody.contains('~'));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}