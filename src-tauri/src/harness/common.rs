//! Shared harness-adapter utilities (Phase 12+).
//!
//! Each harness adapter needs the same low-level primitives:
//! - spawn a CLI binary in the project directory
//! - inject the control-server URL/token into the environment
//! - kill the entire process tree on stop (Windows `taskkill /F /T`, Unix `-TERM`)
//! - write the harness-pack config file with the live control-server URL/token
//! - broadcast events to subscribers
//!
//! This module extracts those primitives so each adapter file focuses on its
//! native wire protocol (stream-json, JSON-RPC, WebSocket, ...) instead of
//! duplicating child-process plumbing.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

use tokio::sync::mpsc;

use crate::harness::event::HarnessEvent;
use crate::harness::HarnessError;

/// Inject `AMAARA_CONTROL_URL` and `AMAARA_CONTROL_TOKEN` into a `Command`.
pub fn inject_control_env(
    cmd: &mut Command,
    control_url: Option<&str>,
    control_token: Option<&str>,
) {
    if let Some(url) = control_url {
        cmd.env("AMAARA_CONTROL_URL", url);
    }
    if let Some(token) = control_token {
        cmd.env("AMAARA_CONTROL_TOKEN", token);
    }
}

/// Spawn `prog` with `args` in `cwd`, piping stdin/stdout and discarding stderr.
/// Injects the control-server env vars before spawning. Returns the child and
/// the piped stdin so the adapter can write native commands.
pub fn spawn_command(
    prog: &Path,
    args: &[String],
    cwd: &Path,
    control_url: Option<&str>,
    control_token: Option<&str>,
) -> Result<(Child, ChildStdin), HarnessError> {
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    inject_control_env(&mut cmd, control_url, control_token);
    // No flashing console window on Windows (creation flag inherited from
    // Command automatically if spawned from a GUI app; explicit for robustness).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| HarnessError::Process(format!("failed to spawn harness: {e}")))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| HarnessError::Process("harness stdin pipe missing".into()))?;
    Ok((child, stdin))
}

/// Kill the entire process tree rooted at `pid`.
/// Windows: `taskkill /F /T /PID <pid>`. Unix: `kill -TERM <pid>`.
pub fn kill_process_tree(pid: u32) {
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output();
    } else {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output();
    }
}

/// Resolve the absolute path to the amaara-mcp workspace directory.
/// Looks first for `AMAARA_MCP_DIR` env var, then the bundled relative path
/// `<src-tauri>/../amaara-mcp` (works both in dev and in an installed app when
/// the workspace is copied next to the binary).
pub fn mcp_workspace_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("AMAARA_MCP_DIR") {
        return PathBuf::from(dir);
    }
    // Default: one level up from the compiled binary's directory.
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("."))
        .parent()
        .unwrap_or(Path::new("."))
        .join("..")
        .join("amaara-mcp")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("amaara-mcp"))
}

/// Write a text config file, creating parent directories as needed.
pub fn write_config(path: &Path, content: &str) -> Result<(), HarnessError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            HarnessError::Process(format!("creating config dir {}: {e}", parent.display()))
        })?;
    }
    std::fs::write(path, content)
        .map_err(|e| HarnessError::Process(format!("writing config {}: {e}", path.display())))
}

// --- User-configured MCP servers (Tools view → harness injection) ----------

/// One user-configured MCP server, normalized for harness config injection.
#[derive(Debug, Clone, PartialEq)]
pub struct UserMcpServer {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    /// Environment as a JSON object (harness configs carry maps, not pairs).
    pub env: serde_json::Map<String, serde_json::Value>,
}

/// Filter + normalize the studio's MCP server list into harness-config
/// entries: enabled servers with a name and a command only.
pub fn user_mcp_servers(list: &[crate::config::McpServerConfig]) -> Vec<UserMcpServer> {
    list.iter()
        .filter(|s| s.enabled && !s.name.trim().is_empty() && !s.command.trim().is_empty())
        .map(|s| UserMcpServer {
            name: s.name.clone(),
            command: s.command.clone(),
            args: s.args.clone(),
            env: s
                .env
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect(),
        })
        .collect()
}

/// a-coder-cli's global agent settings path (`~/.a-coder/cli/agent/settings.json`),
/// honoring the CLI's `A_CODER_CLI_CODING_AGENT_DIR` override.
pub fn aacoder_global_settings_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("A_CODER_CLI_CODING_AGENT_DIR") {
        return Some(PathBuf::from(dir).join("settings.json"));
    }
    let home = if cfg!(windows) {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }?;
    Some(
        home.join(".a-coder")
            .join("cli")
            .join("agent")
            .join("settings.json"),
    )
}

/// Merge the studio's MCP servers into a-coder-cli's global settings
/// (`mcpServers` array, `{name, transport, commandOrUrl, args, env}` shape).
///
/// `configs` is the FULL studio list (enabled and disabled): every configured
/// name is studio-managed — enabled ones are injected, disabled/removed ones
/// are pruned from the file. Entries the user added via the CLI itself
/// (names the studio doesn't manage) are always preserved. Returns Ok(true)
/// when the file changed.
pub fn sync_aacoder_user_mcp_servers(
    settings_path: &Path,
    configs: &[crate::config::McpServerConfig],
) -> Result<bool, HarnessError> {
    // Read + preserve the whole existing settings object.
    let mut root: serde_json::Map<String, serde_json::Value> =
        std::fs::read_to_string(settings_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .and_then(|v: serde_json::Value| v.as_object().cloned())
            .unwrap_or_default();

    let managed: std::collections::HashSet<String> = configs
        .iter()
        .filter(|c| !c.name.trim().is_empty())
        .map(|c| c.name.clone())
        .collect();
    let servers = user_mcp_servers(configs);

    let existing = root
        .get("mcpServers")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    // Keep non-studio entries: anything not named in the studio config AND
    // not carrying our injection marker (the marker lets an empty studio
    // config still prune previously-injected entries).
    let mut merged: Vec<serde_json::Value> = existing
        .into_iter()
        .filter(|entry| {
            let named = entry.get("name").and_then(|v| v.as_str());
            let is_studio = named.map(|n| managed.contains(n)).unwrap_or(false)
                || entry.get("amaaraManaged") == Some(&serde_json::Value::Bool(true));
            !is_studio
        })
        .collect();
    // Append the studio's enabled servers in CLI shape (the extra marker
    // field is ignored by the CLI's settings types but lets later syncs
    // recognize and refresh our entries).
    for s in servers {
        merged.push(serde_json::json!({
            "name": s.name,
            "transport": "stdio",
            "commandOrUrl": s.command,
            "args": s.args,
            "env": s.env,
            "amaaraManaged": true,
        }));
    }

    let changed = root
        .get("mcpServers")
        .map(|old| old != &serde_json::Value::Array(merged.clone()))
        .unwrap_or(true);
    if !changed {
        return Ok(false);
    }
    root.insert("mcpServers".to_string(), serde_json::Value::Array(merged));
    write_config(
        settings_path,
        &serde_json::to_string_pretty(&serde_json::Value::Object(root))
            .map_err(|e| HarnessError::Process(format!("serialize a-coder settings: {e}")))?,
    )?;
    Ok(true)
}

/// Broadcast an event to every live subscriber, dropping closed channels.
pub fn broadcast(subscribers: &mut Vec<mpsc::Sender<HarnessEvent>>, event: HarnessEvent) {
    subscribers.retain(|tx| !tx.is_closed());
    for tx in subscribers {
        let _ = tx.try_send(event.clone());
    }
}

/// Write one JSONL line to a harness's stdin.
pub fn send_json_line(
    stdin: &mut ChildStdin,
    value: &serde_json::Value,
) -> Result<(), HarnessError> {
    let line = format!(
        "{}\n",
        serde_json::to_string(value).map_err(|e| HarnessError::Process(e.to_string()))?
    );
    stdin
        .write_all(line.as_bytes())
        .map_err(|e| HarnessError::Process(format!("write to harness stdin: {e}")))?;
    stdin
        .flush()
        .map_err(|e| HarnessError::Process(format!("flush harness stdin: {e}")))
}

/// A small bundle of child-process state used by every stdio-based adapter.
pub struct ChildState {
    pub started: bool,
    pub stopping: bool,
    pub project_dir: PathBuf,
    pub subscribers: Vec<mpsc::Sender<HarnessEvent>>,
}

impl ChildState {
    pub fn new(project_dir: PathBuf) -> Self {
        Self {
            started: false,
            stopping: false,
            project_dir,
            subscribers: Vec::new(),
        }
    }
}

#[cfg(test)]
mod mcp_injection_tests {
    use super::*;

    fn cfg(name: &str, command: &str, enabled: bool) -> crate::config::McpServerConfig {
        crate::config::McpServerConfig {
            name: name.into(),
            command: command.into(),
            args: vec!["--stdio".into()],
            env: vec![("TOKEN".into(), "secret".into())],
            enabled,
        }
    }

    #[test]
    fn filters_disabled_and_blank_servers() {
        let servers = user_mcp_servers(&[
            cfg("good", "npx", true),
            cfg("off", "npx", false),
            cfg("", "npx", true),
            cfg("no-cmd", "", true),
        ]);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "good");
        assert_eq!(servers[0].command, "npx");
    }

    #[test]
    fn env_pairs_become_json_map() {
        let servers = user_mcp_servers(&[cfg("s", "uv", true)]);
        assert_eq!(
            servers[0].env.get("TOKEN").and_then(|v| v.as_str()),
            Some("secret")
        );
    }

    #[test]
    fn sync_creates_settings_with_cli_shape() {
        let dir = std::env::temp_dir().join(format!("amaara-mcp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");

        let changed =
            sync_aacoder_user_mcp_servers(&path, &[cfg("my-server", "npx", true)]).unwrap();
        assert!(changed);
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let entry = &root["mcpServers"][0];
        assert_eq!(entry["name"], "my-server");
        assert_eq!(entry["transport"], "stdio");
        assert_eq!(entry["commandOrUrl"], "npx");
        assert_eq!(entry["env"]["TOKEN"], "secret");
        assert_eq!(entry["amaaraManaged"], true);
    }

    #[test]
    fn sync_is_idempotent_and_preserves_foreign_entries() {
        let dir = std::env::temp_dir().join(format!("amaara-mcp-idem-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        // Pre-existing settings the CLI owns: another key + another server.
        std::fs::write(
            &path,
            r#"{"theme":"dark","mcpServers":[{"name":"theirs","commandOrUrl":"uvx","transport":"stdio"}]}"#,
        )
        .unwrap();

        let servers = vec![cfg("mine", "npx", true)];
        assert!(sync_aacoder_user_mcp_servers(&path, &servers).unwrap());
        // Second sync — no change.
        assert!(!sync_aacoder_user_mcp_servers(&path, &servers).unwrap());

        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["theme"], "dark");
        let names: Vec<&str> = root["mcpServers"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| e["name"].as_str())
            .collect();
        assert_eq!(names, vec!["theirs", "mine"]);
    }

    #[test]
    fn sync_drops_stale_managed_entries() {
        let dir = std::env::temp_dir().join(format!("amaara-mcp-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let servers = vec![cfg("mine", "npx", true)];
        sync_aacoder_user_mcp_servers(&path, &servers).unwrap();
        // Studio config no longer lists "mine" → the managed entry goes away.
        sync_aacoder_user_mcp_servers(&path, &[]).unwrap();
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["mcpServers"].as_array().map(|a| a.len()), Some(0));
    }

    #[test]
    fn sync_prunes_disabled_servers() {
        let dir = std::env::temp_dir().join(format!("amaara-mcp-dis-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        sync_aacoder_user_mcp_servers(&path, &[cfg("mine", "npx", true)]).unwrap();
        // Same server, now disabled → pruned from the file.
        sync_aacoder_user_mcp_servers(&path, &[cfg("mine", "npx", false)]).unwrap();
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["mcpServers"].as_array().map(|a| a.len()), Some(0));
    }
}
