# NOTES — unknowns & decisions log

Convention (per `BUILD-GAPS.md` §N.55): each entry has a **date**, the
**phase**, a one-line question, and the chosen workaround. Append only; do not
edit history. Unknowns that change early phases must also be raised to the user.

## Resolved log

<!-- 2026-08-24 Phase 0: A.1 decided = Windows-first (dev machine is Windows). -->

## Resolved log (continued)

- A.1 **Windows-first.** Dev machine is Windows; Phase 0 scaffold targets `x86_64-pc-windows-msvc`. macOS/Linux parity deferred to Phase 14, but core code uses no platform-only APIs that would break it.
- A.2 **a-coder-cli is the day-one reference harness** (`AaaCoderCliHarness` first; Claude Code in Phase 12).
- A.3 **Cloud-first default.** `Source` defaults to Navya Cloud; onboarding shows Navya key entry first. sd-server + llama.cpp are opt-in toggles (design.md §8).
- A.4 **Many compositions per studio project.** SQLite has a `compositions` table (project → many); the left rail gets a composition switcher promoted from design.md §12 into v1; timeline/preview is per-composition.
- A.5 **Projects live under OS app-data, relocatable** (`<appdata>/Navya Studio/projects/<id>/…`). "Open folder" relocates them.
- A.6 **Harnesses are user-installed in v1.** The studio spawns the user's installed harness; only a-coder-cli (user's own) is bundled by default. Bundling third-party harnesses is a Phase 14 licensing decision (§N).

### Workarounds / deviations logged here
- **[Risk #32] SQLite backend: `rusqlite` directly, not `tauri-plugin-sql`.** The plan's Phase 1 text says tauri-plugin-sql, but BUILD-GAPS risk #32 recommends rusqlite so Rust owns all DB access and the schema is unit-tested against an in-memory connection. Webview still never touches SQL (surfaced via events).
- **[Risk #33] keyring headless fallback.** `keyring` keeps its platform backend for real app use, but `config::set_secret/get_secret` fall back to a process-local fake + `NAVYA_KEYRING_FAKE=1` so tests and CI run without an OS keychain.
- **esbuild postinstall under pnpm.** pnpm ignores build scripts by default; allowed via `"pnpm": { "onlyBuiltDependencies": ["esbuild"] }` in ui/package.json (the vite/eslint toolchain needs the native esbuild binary).

### Environmental blockers / workarounds documented
- **Tauri 2.6 build-script icon requirement**: This environment's `tauri-build@2.6.3` unconditionally requires a valid Windows DIB-format ICO (`icons/icon.ico`) for resource generation, even with bundle disabled. The DIB format validation fails on minimal/truecolor ICOs generated via standard tools. Workaround: real icons are bundled in Phase 14; M0 dev/test gates may need a `tauri.conf.json` with no bundle targets OR a properly formatted ICO from the Tauri icon generator.
- **ESLint config v9 flat format**: Updated to use `js.configs.recommended` + `react.configs.flat.recommended` per ESLint v9 API changes.

## Phase 14 packaging decisions (2026-08-27)

- **Node sidecar: require host Node in v1.** The plan recommends bundling a
  standalone Node 22, but v1 ships requiring Node ≥ 22 on PATH (the render
  worker and a-coder-cli both spawn `node`/`npx` today, and the onboarding
  hint already says "Install Node.js (≥ 22)"). Bundling standalone Node
  changes the installer by ~50MB/platform and needs a real release build to
  embed and test — deferred until Gate 14 clean-VM runs are possible.
- **sd-server: download-on-first-run** (per plan recommendation, smaller
  installer). Flavor (cpu/cuda/vulkan) comes from `config.sd_backend`; the
  binary lands in the configured `sd_binary_path` (default under app-data).
  SHA256 sidecar files (`.sha256`) are written by `scripts/build-sidecars.*`
  and verified by `sidecar::bootstrap` before use. The release base URL is
  `NAVYA_SD_RELEASE_BASE` (no hardcoded, unverified URL in the binary);
  without it, local generation requires the user to point `sd_binary_path`
  at a binary (BUILD-GAPS: release asset naming is still unresolved).
- **FastMCP `navya-mcp`: via `uv`** (plan-recommended). Only the five
  MCP-capable harnesses need it; the a-coder-cli reference path needs no
  Python. Offline wheel bundling deferred.
- **Bundle targets**: `all` (per-host: msi+nsis on Windows, app+dmg on macOS,
  deb+appimage+rpm on Linux) — matches Gate 14's five-format requirement.
- **externalBin**: kept OUT of the base `tauri.conf.json` because tauri-build
  validates the per-triple files at every compile (breaking dev/CI without the
  staged binary). Release builds that want the binary embedded run
  `scripts/build-sidecars.*` first, then
  `cargo tauri build --config src-tauri/tauri.release.conf.json` (overlay adds
  `bundle.externalBin: [binaries/sd-server]`). When the binary is absent,
  `sidecar/bootstrap.rs` download-on-first-run covers the runtime path.
