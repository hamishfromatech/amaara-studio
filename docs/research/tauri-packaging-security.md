# Navya Studio — Tauri 2 Packaging, Sidecars, Keyring & Security Research

This report pins the exact answers needed to implement Tauri 2 packaging for Navya Studio across Windows, macOS, and Linux, based on:

- `plan/BUILD-GAPS.md` sections **F (Tauri implementation)**, **J (cross-platform specifics)**, and **K.49–K.51 (control-server security)**.
- Context7 docs for `/websites/rs_tauri_2_9_3`, `/tauri-apps/plugins-workspace`, and `/open-source-cooperative/keyring-rs`.
- Local files `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `src-tauri/src/control/server.rs`, and `src-tauri/src/config.rs`.

No code is implemented here; this is the research-backed spec the implementation should follow.

---

## 1. Current Tauri configuration snapshot

`src-tauri/tauri.conf.json` today:

```json
{
  "identifier": "navya-studio",
  "productName": "Navya Studio",
  "version": "0.1.0",
  "build": {
    "beforeDevCommand": "pnpm --filter ./ui dev",
    "devUrl": "http://localhost:5173",
    "frontendDist": "../ui/dist"
  },
  "bundle": {
    "active": true,
    "category": "Developer Tool",
    "targets": ["msi", "nsis"],
    "resources": ["binaries/render-worker.mjs"]
  },
  "app": { "windows": [ { ... } ] }
}
```

Gaps observed:

- No `app.security.csp` value.
- No `bundle.externalBin` entries for harness or ML sidecars.
- No `src-tauri/capabilities/default.json` file exists yet, so `shell:allow-execute` and `shell:allow-sidecar` permissions are missing.
- `bundle.targets` is Windows-only (`msi`, `nsis`). macOS and Linux targets must be added for cross-platform releases.

`src-tauri/Cargo.toml` already depends on `tauri-plugin-shell`, `tauri-plugin-store`, `keyring`, and `rusqlite`, so the required crates are in place.

---

## 2. Cross-platform bundle targets in `tauri.conf.json`

Tauri 2 accepts the following `bundle.targets` values (from `BundleType`):

| Target string | Package produced | Typical platform |
|---------------|------------------|------------------|
| `msi`         | `.msi` installer | Windows          |
| `nsis`        | `.exe` installer | Windows          |
| `dmg`         | `.dmg` disk image| macOS            |
| `app`         | `.app` bundle    | macOS            |
| `appimage`    | `.appimage`      | Linux            |
| `deb`         | `.deb` package   | Linux            |
| `rpm`         | `.rpm` package   | Linux            |

When `targets` is `"all"`, Tauri filters to the OS-relevant subset at build time:

- **Windows**: `msi`, `nsis`
- **macOS**: `app`, `dmg`
- **Linux**: `deb`, `rpm`, `appimage`

Recommended Navya Studio config (Phase 0/2):

```json
"bundle": {
  "targets": ["msi", "nsis", "dmg", "app", "appimage", "deb"]
}
```

`rpm` can be added later when CI is ready to sign packages.

---

## 3. `externalBin` naming and per-target-triple suffixes

In `tauri.conf.json`, sidecars are declared under `bundle.externalBin` using the **base** path/name, relative to `src-tauri`:

```json
"bundle": {
  "externalBin": [
    "binaries/a-coder-cli",
    "binaries/sd-server",
    "binaries/llama-server"
  ]
}
```

For each supported architecture, a binary with the same base name plus `-<target-triple>` must exist at that path. Tauri strips the suffix when copying the binary into the final installer/bundle.

### 3.1 Required file names per target triple

| Platform | Target triple | Sidecar file name (base `my-sidecar`) |
|----------|---------------|----------------------------------------|
| Windows x86_64 | `x86_64-pc-windows-msvc` | `my-sidecar-x86_64-pc-windows-msvc.exe` |
| Windows ARM64 | `aarch64-pc-windows-msvc` | `my-sidecar-aarch64-pc-windows-msvc.exe` |
| macOS Intel | `x86_64-apple-darwin` | `my-sidecar-x86_64-apple-darwin` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `my-sidecar-aarch64-apple-darwin` |
| Linux x86_64 | `x86_64-unknown-linux-gnu` | `my-sidecar-x86_64-unknown-linux-gnu` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | `my-sidecar-aarch64-unknown-linux-gnu` |

The build-time helper script should use `rustc --print host-tuple` to pick the current host triple and rename the downloaded/built binary accordingly.

### 3.2 Where the bundler puts sidecars

- **Windows NSIS / MSI**: external binaries land in `$INSTDIR` (same folder as the main `.exe`). The target-triple suffix is stripped at install time.
- **macOS `.app` / `.dmg`**: external binaries land in `Contents/MacOS` inside the app bundle, suffix stripped.
- **Linux `.deb`**: external binaries land in `/usr/bin`, suffix stripped.
- **Linux `.rpm`**: external binaries land in `/usr/bin`, suffix stripped.
- **Linux `.appimage`**: external binaries are bundled inside the AppImage mount, next to the main executable, suffix stripped.

At runtime, locate the sidecar via `std::env::current_exe().parent()` (or the platform bundle directory). Tauri’s `Command::sidecar("name")` handles this lookup automatically.

---

## 4. `Command::sidecar("name")` resolution: dev vs. bundled

Call the sidecar by its **base name only**, not the full `externalBin` path:

```rust
use tauri_plugin_shell::ShellExt;
let cmd = app.shell().sidecar("a-coder-cli").unwrap();
```

```typescript
import { Command } from '@tauri-apps/plugin-shell';
const cmd = Command.sidecar('a-coder-cli', ['--mode', 'rpc']);
```

### 4.1 Development (`tauri dev`)

- Tauri expects the binary to exist at the `externalBin` base path **with the current host target-triple suffix** (and `.exe` on Windows).
- Example for Windows dev: `src-tauri/binaries/a-coder-cli-x86_64-pc-windows-msvc.exe`.
- Example for macOS Apple Silicon dev: `src-tauri/binaries/a-coder-cli-aarch64-apple-darwin`.
- If the suffix-named binary is missing, `sidecar()` will fail to resolve.

### 4.2 Bundled/production

- The bundler has stripped the suffix, so the binary is next to the main executable.
- `Command::sidecar("a-coder-cli")` resolves to that stripped binary via the bundle layout.
- No PATH lookup is required.

### 4.3 Fallback for non-bundled / downloaded binaries

For binaries downloaded after install (e.g., GPU-flavored `sd-server` or a first-run harness), do **not** use `Command::sidecar()`. Use `Command::create()` with the absolute path and declare that path in the capability ACL instead (see §6).

---

## 5. Capabilities and ACL scope entries for shell and localhost origins

Create `src-tauri/capabilities/default.json`.

### 5.1 Sidecar/harness execution

A minimal capability for the sidecars declared above:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Main window shell permissions",
  "windows": ["main"],
  "permissions": [
    "core:default",
    {
      "identifier": "shell:allow-execute",
      "allow": [
        {
          "name": "a-coder-cli",
          "sidecar": true,
          "args": [
            "--mode",
            "rpc",
            "--cwd",
            { "validator": "^.*$" }
          ]
        },
        {
          "name": "sd-server",
          "sidecar": true,
          "args": true
        },
        {
          "name": "render-worker",
          "cmd": "node",
          "sidecar": false,
          "args": [
            { "validator": "^.*render-worker\.mjs$" },
            { "validator": "^.*$" }
          ]
        }
      ]
    }
  ]
}
```

Key points:

- `name` must match the base name used in `Command::sidecar()`.
- `sidecar: true` restricts the command to the bundled sidecar layout.
- `args` can be a boolean (`true` = any args), a fixed array, or an array containing `{ "validator": "<regex>" }` objects. Use validators to prevent argument injection from the webview.
- For downloaded binaries, use `"sidecar": false` and either `"cmd": "/absolute/path"` or `"name": "downloaded-binary"` with a validator on the absolute path.

### 5.2 Localhost / control-server origin access

The webview itself does not call the control server (the harness extension does, outside the webview), but the **HyperFrames preview iframe** will load from a localhost origin. Grant that origin access via the `localhost` plugin or a remote capability entry.

If the preview server runs at `http://localhost:<port>` or `http://127.0.0.1:<port>`, add a remote capability:

```json
{
  "identifier": "preview-remote",
  "description": "Allow HyperFrames preview iframe origin",
  "windows": ["main"],
  "permissions": [
    {
      "identifier": "core:default"
    }
  ],
  "remote": {
    "urls": ["http://localhost:*", "http://127.0.0.1:*"]
  }
}
```

Alternatively, use `tauri_plugin_localhost` and add the capability dynamically in Rust with `CapabilityBuilder::new("localhost").remote(url).window("main")`.

---

## 6. Windows path/quoting and console-window suppression

### 6.1 Path handling for Node/HyperFrames

- **Tauri `Command::create()` / `Command::sidecar()` pass arguments as an array**, not a shell string. This is the safe path: no shell interpolation occurs, so backslashes in Windows paths are preserved literally.
- When paths are passed to a Node script or HyperFrames CLI that expects POSIX-style paths, normalize backslashes to forward slashes (`D:\project\foo` → `D:/project/foo`) **before** placing them in the args array. Do not rely on shell quoting to preserve backslashes.
- If a command must be built as a single string for a shell interpreter (rare; avoid), use `std::process::Command` with proper Windows quoting or the `shell-escape`/`regex` validator in the capability.

### 6.2 Console-window suppression

Tauri’s shell plugin does not expose a direct “hide console window” toggle. To avoid flashing `cmd.exe` windows when spawning console-subsystem sidecars (Node, Python, `a-coder-cli`, etc.) on Windows:

- Build the Rust **main** binary with:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
```

- For each child process on Windows, set the `CREATE_NO_WINDOW` creation flag via `std::os::windows::process::CommandExt`:

```rust
use std::os::windows::process::CommandExt;
const CREATE_NO_WINDOW: u32 = 0x08000000;
std::process::Command::new("node")
    .arg("render-worker.mjs")
    .creation_flags(CREATE_NO_WINDOW)
    .spawn();
```

- If spawning from Rust through `tauri_plugin_shell`, use the plugin for event streaming but wrap the launch in a small Rust helper that configures Windows flags before handing the process to Tauri, or ensure the spawned binary is a GUI-subsystem binary.

Recommended Phase 2/7 work item: add a `spawn_hidden(cmd, args)` helper in `src-tauri/src/process.rs` that applies `CREATE_NO_WINDOW` on Windows and is used for all harness, render-worker, and ML sidecar spawns.

---

## 7. `keyring` crate backends and headless/CI fallback

The `keyring` crate (version `>=2`, currently used as `>=2` in `Cargo.toml`) provides cross-platform secret storage.

### 7.1 Platform backends

| OS | Default backend | Credential store |
|----|-----------------|--------------------|
| Windows | Windows Credential Manager | `use_windows_native_store` / automatic |
| macOS | macOS Keychain Services | `use_apple_keychain_store` |
| Linux | Secret Service (D-Bus) via zbus | `use_zbus_secret_service_store` |

Default usage:

```rust
let entry = keyring::Entry::new("navya-api", "api-key")?;
entry.set_password("sk-...")?;
let pw = entry.get_password()?;
```

`Entry::new(service, user)` is the portable API; the crate selects the platform backend automatically.

### 7.2 Headless / CI fallback

The current `src-tauri/src/config.rs` already implements the right pattern:

- Real OS keyring is used in production.
- A thread-local in-memory `FakeKeyring` is enabled via `set_fake_keyring(true)` for tests and headless/CI runs.
- The env var `NAVYA_KEYRING_FAKE` (or a test-only call) can force the fake backend.

Context7 also documents `use_sample_store(&HashMap::new())` as a Linux headless fallback. For Navya Studio, keep the existing `FakeKeyring` approach; it is deterministic and avoids depending on a D-Bus session bus in CI.

Important platform notes:

- Linux Secret Service requires a running D-Bus daemon and an unlocked collection. In headless/SSH sessions it typically fails.
- `keyutils`/kernel keyring may time out without a TTY, which is why the sample/fake store is recommended for tests.

### 7.3 Secret scope

Store only these items in the keyring:

- `navya-api` / `api-key` — Navya Cloud API key.
- `navya-control-token` / `token` — ephemeral control-server bearer token.
- Future: provider BYOK keys, llama.cpp/sd-server credentials.

Never write secrets to project files, `~/.env`, or the `tauri-plugin-store` JSON settings file. The existing test `secret_round_trip_via_fake_backend` covers the fallback path.

---

## 8. Control-server CSP: allow `127.0.0.1` and the HyperFrames preview iframe origin

Add an `app.security.csp` block to `tauri.conf.json`.

```json
"security": {
  "csp": {
    "default-src": "'self' customprotocol: asset:",
    "connect-src": "'self' ipc: http://ipc.localhost http://127.0.0.1:*",
    "img-src": "'self' asset: http://asset.localhost blob: data: http://127.0.0.1:*",
    "media-src": "'self' blob: http://127.0.0.1:*",
    "frame-src": "'self' http://localhost:* http://127.0.0.1:*",
    "style-src": "'unsafe-inline' 'self'",
    "script-src": "'self' 'unsafe-inline'",
    "font-src": "'self' asset: http://asset.localhost"
  }
}
```

Rationale:

- `connect-src` allows the webview to fetch from the local control server on any ephemeral port (`127.0.0.1:*`). Tauri appends its own nonces/hashes to this directive at build time.
- `frame-src` allows the HyperFrames preview iframe to load from a localhost preview server. If the preview server uses a fixed port, prefer that exact port over a wildcard.
- Avoid `*` origins or `unsafe-eval`.

---

## 9. Control-server security best practices

Current `src-tauri/src/control/server.rs` already does several things correctly:

- Binds `127.0.0.1:0` (ephemeral loopback only).
- Generates a random 32-byte hex bearer token prefixed with `navya-`.
- Writes the token to the OS keyring before spawning the harness.
- Tests reject missing and wrong tokens with `401 Unauthorized`.

Gaps and required hardening:

### 9.1 Remove permissive CORS

The current router uses:

```rust
.layer(CorsLayer::permissive())
```

This is unsafe for a local-only server. Replace with **no CORS layer at all** or a very restrictive one. The control server is only accessed from the same machine, so CORS is unnecessary. If a CORS layer is kept for local development, it must be gated to `127.0.0.1` origins only and never enabled for non-loopback interfaces.

### 9.2 Enforce loopback-only binding

Always bind like this:

```rust
let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
```

Never bind to `0.0.0.0` or `[::]`. After binding, verify `listener.local_addr()` still resolves to a loopback IP.

### 9.3 Bearer-token auth middleware

Every `POST /tool/:name` request must carry:

```
Authorization: Bearer <control-token>
```

Middleware should:

1. Extract the `Authorization` header.
2. If missing, return `401 Unauthorized`.
3. If it does not start with `Bearer `, return `401`.
4. Compare the token to the stored token using a constant-time comparison (`subtle::ConstantTimeEq` or similar).
5. If mismatch, return `401`.
6. Only then dispatch the tool.

The existing tests already assert this behavior, but the production router currently lacks a dedicated middleware; the token check is inside the dispatch function. Move it to a `ValidateToken` layer so all tool routes share the same enforcement.

### 9.4 No CORS for non-loopback

Because the server never listens on non-loopback addresses, CORS configuration is moot in production. The safest configuration is **no CORS layer**. If a frontend workflow genuinely requires it during `tauri dev`, add the layer only under `#[cfg(dev)]` and restrict origins to `http://localhost:*` and `http://127.0.0.1:*`.

### 9.5 WebSocket `/events`

The event WebSocket currently does not authenticate. Options:

- Require the bearer token as a query parameter (`/events?token=...`) and reject the upgrade if missing/invalid.
- Or, accept the WebSocket upgrade only from `127.0.0.1` and require the token on the first client message.

Because the server is loopback-only and the token is written to the keyring, the risk is low, but adding token validation on the WebSocket path is recommended defense in depth.

### 9.6 Write token to keyring before spawning the harness

Current order in `server.rs::launch`:

1. Generate token.
2. Write token to keyring.
3. Build router and store URL/token in app state.
4. Spawn server.
5. Return handle.

This order is correct. The adapter must not start the harness until the handle is returned and the URL/port is stable.

---

## 10. Sidecar bootstrap / downloader patterns in Tauri

Tauri 2 has no built-in “download a missing sidecar on first run” feature. Navya Studio needs a custom bootstrap flow for binaries that are too large or license-restricted to bundle (e.g., GPU-specific `sd-server`, optional `llama-server`, harnesses that the user does not pre-install).

### 10.1 Recommended bootstrap flow

1. **Manifest**: Ship a `sidecars.json` manifest in `resources/` listing per target-triple:
   - `url` (HTTPS)
   - `sha256`
   - `size` (optional)
   - `signature` (optional, minisign)
2. **Check at startup**: Compare the expected version against the downloaded binary in the app data dir (`app.path().app_data_dir()`).
3. **Download if missing/outdated**: Use `reqwest` (Rust) or `@tauri-apps/plugin-upload` `download()` to fetch the binary to a temp file in app data.
4. **Verify**: Compute SHA-256 and compare to the manifest. If a signature is provided, verify it with `minisign` / `ed25519`.
5. **Atomically replace**: Rename temp file to final name and set executable bit on Unix (`chmod +x`).
6. **Record state**: Store the downloaded version in `tauri-plugin-store`.
7. **Spawn**: Use `Command::create()` with the absolute downloaded path, or add the path to `externalBin` only if it is bundled at build time.

### 10.2 Progress UI

- Use a Tauri `Channel` from Rust to emit progress events to the webview:
  - `Started { content_length }`
  - `Progress { chunk_length }`
  - `Finished`
  - `Error { message }`
- Render a modal/overlay in the React UI during first-run download.
- Alternatively, emit Tauri events (`app.emit("sidecar-download-progress", ...)`) and subscribe in the frontend.

### 10.3 Security of downloaded sidecars

- Download only over HTTPS with pinned or system-trusted TLS.
- Always verify SHA-256 before execution.
- Prefer signed binaries (minisign) and verify the signature against a public key embedded in the app bundle.
- Do not allow arbitrary user-specified URLs for auto-downloaded sidecars; only download from the manifest.
- Store downloaded binaries in the OS app-data dir, not the project directory, to avoid leaking harness binaries into user-facing folders.

### 10.4 Two sidecar categories

| Category | How to declare | Example |
|----------|---------------|---------|
| **Bundled sidecars** | `bundle.externalBin` + `Command::sidecar()` | `a-coder-cli` (if bundled), render helper |
| **Downloaded sidecars** | `resources/sidecars.json` + `Command::create(abs_path)` | GPU-specific `sd-server`, optional `llama-server` |

For user-installed harnesses (the Phase 0 decision), use `Command::create("a-coder-cli", args)` and rely on the user’s PATH, with a capability entry allowing the bare command name and validating required args.

---

## 11. Implementation checklist (derived from this report)

1. Expand `tauri.conf.json`:
   - Add `bundle.targets`: `["msi", "nsis", "dmg", "app", "appimage", "deb"]`.
   - Add `bundle.externalBin` entries for bundled sidecars.
   - Add `app.security.csp` allowing `127.0.0.1:*` and localhost preview origins.
2. Create `src-tauri/capabilities/default.json` with `shell:allow-execute` entries for each sidecar and downloaded binary, using regex validators on args/paths.
3. Build per-target-triple sidecar binaries and place them in `src-tauri/binaries/` with the correct suffixes.
4. Add `spawn_hidden` Windows helper that applies `CREATE_NO_WINDOW`.
5. Harden `control/server.rs`:
   - Remove `CorsLayer::permissive()`.
   - Add a token-auth middleware/layer.
   - Optionally authenticate `WS /events` upgrades.
6. Keep the existing `FakeKeyring` fallback for headless/CI, and ensure all secrets round-trip through `keyring` only.
7. Implement the first-run sidecar bootstrap manifest + downloader with SHA-256 verification and progress channel.

---

## 12. Sources

- Tauri 2.9.3 docs (`/websites/rs_tauri_2_9_3` via Context7): `Config` struct, bundle types, `externalBin` behavior.
- Tauri docs (`/tauri-apps/tauri-docs` via Context7): sidecar guide, NSIS/deb/rpm binary placement, CSP guide, shell plugin capabilities.
- Tauri plugins workspace (`/tauri-apps/plugins-workspace` via Context7): `tauri-plugin-shell` `CommandOptions`, `tauri-plugin-upload` download progress, `tauri-plugin-updater` signature verification.
- `keyring-rs` (`/open-source-cooperative/keyring-rs` via Context7): native stores, sample/mock store for headless environments.
- Local files: `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `src-tauri/src/control/server.rs`, `src-tauri/src/config.rs`, `plan/BUILD-GAPS.md` §F/J/K.
