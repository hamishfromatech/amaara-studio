# Navya Studio

An AI content-creation studio — a desktop app where an agent writes the script, generates the assets, authors the composition, and renders the video. You bring the idea; the studio does the rest.

Built as a **Tauri 2 desktop app** (Rust core + React webview). An AI agent — the *harness* — orchestrates everything: it writes scripts and storyboards, generates images, assembles **HyperFrames / Remotion** compositions, and renders video locally. Intelligence comes from **Navya Cloud** (OpenAI-compatible, remote) or **fully local models** (llama.cpp + stable-diffusion.cpp) — one switch, no code changes.

> **Status:** active development, milestone-driven (see [`plan/plan.md`](plan/plan.md)). Windows-first today; the Rust core is cross-platform and CI builds Windows + macOS.

## How it works

```
 React UI (webview)  ◄─IPC─►  Rust core (Tauri)  ─spawns─►  Harness agent (AI)
                                  │                            │
                          SQLite · keyring ·          tool calls (MCP / extension)
                          config · events                     │
                                  ▼                           ▼
                     Render pipeline · sd-server      loopback control server (Rust)
```

- **UI** (`ui/`) — React + Vite + Tailwind SPA: chat with the agent, project/composition management, model & source pickers, timeline/preview, render queue. Pure view layer — all state comes from the Rust core via `invoke()` and events.
- **Rust core** (`src-tauri/`) — the desktop binary: SQLite storage, settings + OS-keyring secrets, sidecar supervision, and a loopback HTTP/WebSocket **control server** that owns all tool logic.
- **Harness layer** (`src-tauri/src/harness/`) — `trait Harness` normalizes any agent CLI into one interface (`prompt` / `steer` / `abort` / `set_model` + a unified event stream). Adapters ship for **a-coder-cli** (reference), Claude Code, OpenAI Codex, Antigravity, Hermes, and OpenClaw. Capability gaps degrade explicitly, never silently.
- **Tools** — written once in the Rust control server, exposed to the agent as MCP tools (`navya-mcp/`, FastMCP) or an a-coder-cli extension: `generate_image`, `render_to_video`, `set_generation_source`, `get_project_state`, `snapshot`, …
- **Sidecars** — the render worker (`src-tauri/binaries/render-worker.mjs`, Node 22) and the local image server (stable-diffusion.cpp `sd-server`, downloaded on first run) run as supervised, isolated processes.

A full write-up, including the reasoning behind every decision, lives in [`plan/ARCHITECTURE.md`](plan/ARCHITECTURE.md).

## One prompt, end to end

1. You type *"make a 30s faceless explainer about black holes"* and pick Cloud or Local.
2. The harness scaffolds a HyperFrames project and authors the composition (all tool activity streams into the chat view).
3. `generate_image` routes to Navya Cloud or the local `sd-server` depending on your source toggle.
4. `render_to_video` queues the render; progress streams to the render panel.
5. The finished video lands in the project folder — reveal it in Explorer from the app.

## Getting started

**Prerequisites**

- Windows (macOS/Linux parity is planned; the Rust core uses no Windows-only APIs)
- [Rust](https://rustup.rs) stable + `rustfmt`, `clippy` (pinned via `rust-toolchain.toml`)
- [Node.js](https://nodejs.org) ≥ 22 and [pnpm](https://pnpm.io) 10 (`corepack enable`)
- An agent harness on PATH — [a-coder-cli](https://github.com/hamishfromatech) is the reference (others are picked per project in the UI)
- A Navya API key (entered on first launch; stored in the OS keyring)

**Run it:**

```bash
pnpm install
pnpm dev        # Vite dev server + Tauri app, live-reloading
```

**Checks used by CI:**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm build   # inside ui/, or via the root scripts
```

**Local image generation (optional):** build or point the studio at an `sd-server` binary via `scripts/build-sidecars.*` — CUDA, Vulkan, and CPU flavors are supported. Cloud image/video generation needs nothing extra.

## Repository layout

```
├── src-tauri/        # Rust core: commands, control server, harness adapters,
│                     #   Navya/SD/render clients, SQLite store, sidecar mgmt
├── ui/               # React SPA (chat, projects, models, timeline, renders)
├── navya-mcp/        # FastMCP (Python) tool server → proxies to the control server
├── harness-pack/     # per-harness config the studio materializes (skills,
│                     #   prompts, extensions, MCP wiring)
├── crates/           # auxiliary Rust crates
├── tools/  scripts/  # dev tooling + sidecar build scripts
├── plan/             # ARCHITECTURE.md, design.md, plan.md, research notes
└── docs/             # research pass findings
```

## License

Dual-licensed: **MIT** for individuals and organizations under the revenue threshold, or **The A-Tech Corporation License** for larger enterprises' commercial use — see [LICENSE](LICENSE).