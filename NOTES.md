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

## Full E2E verification pass on macOS (2026-09-15)

Ran every CI gate + the headless and live E2E suites on Apple Silicon
(macOS — first non-Windows verification of this tree). 146 Rust tests green;
all UI gates green; release build compiles. Issues found and fixed:

- **macOS builds failed: `generate_context!` requires a PNG icon on
  non-Windows targets** (tauri-codegen falls back to the hardcoded
  `icons/icon.png` when no `.ico` applies; only `icon.ico`/`icon.svg`
  existed). Added `icons/icon.png` (512px, brand ring) and listed it first in
  `tauri.conf.json` bundle icons.
- **clippy `-D warnings` failed on a current toolchain** (rustc/clippy
  1.97.1; the Windows dev box ran an older clippy): fixed 13 lints —
  `option_as_ref_deref`, `unnecessary_to_owned` ×2, `bool_assert_comparison`
  ×2, `len_zero`, `let_and_return`, `doc_lazy_continuation`,
  `too_many_arguments` (allow-commented: the arg count is the Tauri wire
  contract), `empty line after doc comment`, unused import; `llama.rs`
  (built, not yet wired into app state) got a documented module-level
  `#![allow(dead_code)]` until its wiring phase.
- **`cargo fmt --check` failed tree-wide** (pre-rustfmt drift). Applied
  `cargo fmt`; tree is now format-clean on the pinned stable.
- **RESOLVED a long-standing loose end**: `live_rpc_models_round_trip`
  "hung against the real a-coder-cli". Root cause was NOT first-run trust
  prompts — the test used the default current-thread `#[tokio::test]` while
  the adapter's stdout reader blocks in `read_line` inside a spawned task,
  deadlocking the single-worker runtime before the command was ever sent.
  Switched to `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`
  (same lesson the e2e stub test already documented). The live round-trip now
  completes in ~3s against a fully configured a-coder-cli and returns the
  real model catalog — validating the adapter framing against the live CLI.
- Verified on this machine additionally: render-worker.mjs parses under Node
  22; navya-mcp FastMCP server imports and registers all 7 tools; release
  `cargo build --release` succeeds (CI build-matrix macOS equivalent).

Still manual/host-dependent gates: real video render (needs HyperFrames +
FFmpeg + a project), sd-server image generation (needs staged weights),
packaged installers, and the `--ignored` live test remains a manual gate by
design (needs a configured CLI).

## Rebrand: Navya Studio → Amaara Studio (2026-09-15)

Decision: full rename across product, code, and infrastructure; the remote
service (formerly Navya Cloud) is now **Amaara Cloud**. Clean break on data —
existing installs under the old identity (identifier `navya-studio`,
`navya.db`, `navya_base_url` config key, old keyring services) are NOT
migrated; a fresh install starts empty.

Renamed (779 occurrences / 66 living files):
- Product/UI strings, README, ARCHITECTURE/design docs.
- Tauri identifier `amaara-studio`, productName "Amaara Studio", binary
  `amaara-studio`, data dir follows the identifier.
- Crates: `amaara-studio`, `amaara-tools` (dir + package names).
- Python: `amaara-mcp/` dir + `amaara_mcp` package, FastMCP server name
  `amaara-studio-tools`.
- Env-var contracts (renamed together across adapter/extension/MCP/stubs/
  scripts so every component stays in sync): `AMAARA_CONTROL_URL/TOKEN`,
  `AMAARA_SD_RELEASE_BASE`, `AMAARA_SD_SERVER_BIN`, `AMAARA_SD_RELEASE_URL`,
  `AMAARA_MCP_DIR`, `AMAARA_AACODER_BIN`, `AMAARA_SIDECAR_*_BIN`,
  `AMAARA_KEYRING_FAKE`.
- Config: keyring services `amaara-api-key` / `amaara-control-token`,
  `amaara_base_url`, default model `amaara/auto`, db file `amaara.db`,
  harness-pack extension/skill filenames, sidecar stub binaries
  (`amaara-harness-stub`, `amaara-render-stub`, `amaara-sd-stub`).

Deliberately NOT rewritten (append-only history, per this log's convention):
`NOTES.md`, `plan/NOTES.md`, `plan/plan.md`, `plan/BUILD-GAPS.md`,
`plan/RESEARCH.md`, and the research snapshot `docs/research/` (file renamed
to `amaara-cloud-api.md`, content left as the dated record of the API at
research time — its `X-Navya-*` header names describe the service as
observed pre-rebrand).

All gates re-verified green after the rename: fmt, clippy -D warnings,
146 Rust tests (incl. headless e2e), UI typecheck/lint/build, release build
(`target/release/amaara-studio`), FastMCP import, render-worker parse.

## First live product trial on macOS (2026-09-15, post-rebrand DMG)

Installed `Amaara Studio_0.1.0_aarch64.dmg` to /Applications and drove the
real prompt → harness → tools flow. Findings:

- **Harness model selection now works end to end** (post 074c265): the CLI
  session records `model_change → ollama-cloud/glm-5.3-flash` from the
  studio picker; the agent ran real turns on the user's chosen model.
- **macOS GUI PATH**: `open -a` inherited a full dev PATH on this machine
  (launchd global env). On a vanilla Mac the Finder-launch PATH is minimal
  (/usr/bin:/bin:...) — Gate 14 clean-VM smoke must verify harness/node
  discovery there; a login-shell PATH refresh may be needed.
- **Rebuilt-binary keychain gotcha**: a rebuilt ad-hoc binary can't silently
  rewrite the control-token keychain item the previous binary created — the
  control-server launch task blocked on the security prompt (app boots but
  no control server, empty log). Deleting the stale
  `amaara-control-token` item unblocks. Dev-loop only; real installs sign
  consistently.
- **Bash tool hang (fixed upstream in a-coder-cli 4adfa19d7)**: the agent's
  `python3` wedged during interpreter startup — anaconda site-packages has
  a broken `__editable__.ecommerce_admin-1.0.0.pth` that intermittently
  hangs python init. The CLI's bash tool had NO default timeout, so the
  turn hung forever and the studio could only show a silent stall. Killing
  the wedged child unblocked the tool; the agent adapted immediately
  (switched to node -e). CLI now defaults bash timeout to 120s (takes
  effect on next CLI rebuild/reinstall). The broken .pth should be removed
  from anaconda site-packages machine-side.
- Studio log stays too quiet around harness spawn/exit (only "model set"
  and "control server started" lines). The EOF error broadcasts to chat
  but isn't tracing-logged; consider logging harness lifecycle (spawn,
  exit code, stderr lines already log at warn).
- **Follow-up (same day):** the broken `__editable__.ecommerce_admin-1.0.0.pth`
  was uninstalled from anaconda (`pip uninstall ecommerce-admin`) — python
  startup is back to ~20ms with no .pth errors; the agent's python tool
  shape verified clean.
