# Navya Studio — Agent Notes

Persistent log of unknowns, workarounds, and decisions. Each entry:
`YYYY-MM-DD | Phase | Question | Chosen workaround/answer`.

## 2026-08-26 | Phase 13/Research | Harness protocol details for adapters
All six harness adapters implemented from `plan/RESEARCH.md` and the
`harness-pack` configs. Only `claude` is installed on the dev machine; the
other five are best-effort based on docs and compile cleanly. End-to-end
verification requires installing `codex`, `agy`, `hermes`, `openclaw`.

## 2026-08-26 | Phase 13 | FastMCP server API drift
`navya-mcp/server.py` used an old FastMCP API (`ctx.report_progress(string)`).
Updated to FastMCP v4 (`ctx.report_progress(progress, total, message)`),
pinned `fastmcp>=4.0.0b3` and `httpx>=0.28.0` in `requirements.txt` and
`pyproject.toml`. Added pytest dev deps and `asyncio_mode = auto`.

## 2026-08-26 | Phase 9 | Local LLM sidecar
`src-tauri/src/sidecar/llama.rs` had been deleted. Restored and implemented:
`LlamaConfig`, `LlamaServer::ensure_running/start/stop/is_healthy`, health
polling, `/props` tool-call capability probe, `StudioError::From<String>`
helper, and `errors` module exported from `lib.rs`. Still needs real
`llama-server` binary + model to verify end-to-end.

## 2026-08-26 | Phase 14 | Cross-platform packaging not started
`tauri.conf.json` only targets Windows (`msi`, `nsis`). macOS/Linux bundles,
`externalBin` entries, sidecar bootstrap/downloader, and first-run dependency
fetch are not yet implemented. Research captured in
`docs/research/tauri-packaging-security.md`.

## 2026-08-26 | Phase 15 | Hardening stub
`src-tauri/src/errors.rs` has typed `ErrorCode`/`StudioError` but no
structured logging, telemetry, headless e2e, or plaintext-key leak test yet.

## 2026-08-26 | Phase 16 | UI polish partial
Theming/keyboard/onboarding exist in `design.md` but are not fully implemented
in the UI. Status strip uses live data; composition switcher and timeline track
parser are not wired end-to-end.

## 2026-08-26 | Phase 8 | Local image generation partial
`sd-server` research captured in `docs/research/stable-diffusion-local.md`.
Rust sidecar lifecycle not yet implemented; cloud image gen path depends on
Navya API key not yet available on the build machine.

## 2026-08-26 | Phase 7/10 | Render + timeline
Render sidecar (`render-worker.mjs`) and queue exist. HyperFrames CLI exact
JSON progress shape is still unknown; need to run `npx hyperframes render`
live to confirm. Timeline HTML parser based on `data-*` attributes is
researched but not implemented.
