# Navya Studio — Architecture

A Tauri desktop app that is a content-creation studio powered by AI. An agent
(harness) orchestrates the work: it writes scripts/storyboards, generates
images, and assembles + renders video. Intelligence comes from **Navya Cloud**
AI models (remote, OpenAI-compatible) **or** local models (local LLM +
stable-diffusion.cpp). Video is authored in **HyperFrames/Remotion** and
rendered locally.

This document is the synthesis of the context7 research pass. See
`RESEARCH.md` for the raw findings behind each decision.

---

## 1. The big picture

```
 ┌──────────────────────────────────────────────────────────────────────┐
 │                       Navya Studio (Tauri desktop app)              │
 │                                                                      │
 │  ┌──────────────────────┐        ┌───────────────────────────────┐  │
 │  │  Web UI (webview)     │  IPC   │  Rust core (tauri::Builder)    │  │
 │  │  React + Tailwind     │◄──────►│  • project/asset/state mgmt   │  │
 │  │  • chat (agent)       │ invoke │  • SQLite (projects, renders) │  │
 │  │  • timeline / preview │        │  • settings + secrets (keyring)│  │
 │  │  • render queue       │        │  • sidecar supervisor          │  │
 │  │  • model/source picker│        │  • trait Harness adapters      │  │
 │  └──────────────────────┘        └───────────┬───────────────────┘  │
 │                                                │ spawns / pipes      │
 │  ┌─────────────────┐  ┌─────────────────┐  ┌───▼────────────┐        │
 │  │ Harness sidecar │  │ Render sidecar  │  │ SD.cpp sidecar │        │
 │  │ (one of six,   │  │ Node 22 +       │  │ sd-server      │        │
 │  │  e.g. a-coder- │  │ hyperframes /   │  │ (or sd-cli)    │        │
 │  │  cli --mode    │  │ @remotion/…     │  │ HTTP :7860     │        │
 │  │  rpc)         │  │                 │  │                │        │
 │  └────────┬───────┘  └─────────────────┘  └────────────────┘        │
 │           │ native JSON(RPC) over stdio (or WebSocket for OpenClaw)    │
 └───────────┼──────────────────────────────────────────────────────────┘
             │ tool calls route to the studio MCP tool server, which calls:
     ┌───────┴────────┬─────────────────────┐
     ▼                ▼                     ▼
 Navya Cloud      llama.cpp server      sd-server (local)
 /v1/chat,        (local LLM,           local image gen
 /v1/images,      OpenAI-compatible)
 /v1/videos
```

Five roles, each a separate process the Rust core supervises:

| Process | What it is | Why a sidecar/process |
|---|---|---|
| **Web UI** | Bundled React SPA in the Tauri webview | The user-facing surface |
| **Harness sidecar** | One of six (a-coder-cli by default). The agent brain — tools, skills, prompt templates, the HyperFrames skills. Drives the whole studio. | The studio is harness-agnostic; the user picks the harness. |
| **MCP tool server** | The studio's custom tools (`generate_image`, `render_to_video`, `list_local_models`, `set_generation_source`, …) exposed over stdio MCP | Written once, consumed by **five of six** MCP-capable harnesses (a-coder-cli uses a TS extension instead — see §2.1). |
| **Render sidecar** | A bundled Node 22 runtime that runs `npx hyperframes …` and `@remotion/renderer` | HyperFrames/Remotion need Node + headless Chrome + FFmpeg. Isolated from the agent so a render crash can't kill the conversation. |
| **SD.cpp sidecar** | `sd-server` (HTTP) or `sd-cli` (one-shot) | Local image generation. Started lazily when the user picks "local" generation. |

Navya Cloud and a local llama.cpp server are **not** sidecars — they are remote
(or user-run) OpenAI-compatible endpoints the harness talks to over HTTP.

---

## 2. Why this shape (decisions from the research)

### 2.1 Harness layer: a pluggable adapter over six real harnesses
The concept says the harness is agnostic — a-coder-cli, claude code, hermes,
openclaw, openai codex, antigravity cli. Researching each (see `RESEARCH.md`)
turns up two structural facts that fix the design:

1. **MCP covers five of six — a-coder-cli is the exception.** Claude Code,
   Codex, Antigravity, Hermes, and OpenClaw all support MCP servers for custom
   tools. a-coder-cli **intentionally has no built-in MCP** (`docs/usage.md`:
   "It intentionally does not include built-in MCP"); it uses TypeScript
   extensions + `customTools`/`pi.registerTool`. So the studio tools are a
   **single Rust backend** exposed two ways: (a) a stdio **MCP server** built with **FastMCP** (Python) for the five
   MCP harnesses, and (b) a thin **a-coder-cli TypeScript extension**.
   Both are thin proxies to a single **Rust control server** (loopback HTTP)
   that owns the tool logic. Tool logic is written once; only the transport
   shim differs per family. (FastMCP chosen over hand-rolling MCP stdio in
   Rust for spec-correct framing + progress; see `RESEARCH.md`.)
2. **Five of six expose a bidirectional stdio JSON-RPC/NDJSON stream** with
   streaming events and (mostly) mid-stream control. OpenClaw is the outlier —
   gateway/WebSocket-centric — and joins via `openclaw mcp serve` or its WS
   SDK.

So `trait Harness` in Rust is a **protocol translator**, not a shared wire
format. Each adapter: (a) spawns/connects the harness, (b) injects our MCP
server into the harness's MCP config, (c) translates normalized Navya commands
(`prompt` / `steer` / `abort` / `set_model`) to the native protocol,
(d) emits a normalized `HarnessEvent` stream the UI renders, (e) routes the
harness's approval/permission requests to native Tauri dialogs.

#### Per-harness integration (grounded in research)

| Harness | Native transport the adapter speaks | Custom tools | Mid-stream steer | Approvals → UI |
|---|---|---|---|---|
| **a-coder-cli** (reference) | `--mode rpc` stdio JSONL | ext/customTools (no MCP) | yes (`steer`) | extension-UI dialog sub-protocol |
| **Claude Code** | `-p --output-format stream-json --verbose --include-partial-messages`; `--input-format stream-json` for streaming input | MCP + hooks | yes (input stream + interrupt) | `PreToolUse` / `PermissionRequest` allow/deny |
| **OpenAI Codex** | `codex app-server` JSON-RPC (`thread/start`, `command/exec`, `dynamicTools`) | MCP + `dynamicTools` | yes (thread) | `approvalPolicy` |
| **Antigravity `agy`** | `agy -p --output-format stream-json` (NDJSON) | MCP only | **no** → degrade to abort + re-prompt | MCP / permissions |
| **Hermes** | `tui_gateway` stdio JSON-RPC (`gateway.ready` …) | MCP + Python plugins | yes | plugin hooks |
| **OpenClaw** | WebSocket gateway (`ws://…:18789`) or `openclaw mcp serve` | MCP + `tools.invoke` | yes | `tools.invoke` confirm request/report |

Capability differences degrade **explicitly**, not silently:

- A harness without mid-stream steer (Antigravity print mode) implements
  `steer` as `abort` + re-prompt; the UI shows it as a coarse interruption.
- A print-mode harness runs **one process per turn**; the adapter caches the
  working dir + session id across processes.
- OpenClaw's adapter speaks WebSocket, or — since OpenClaw is itself a
  gateway — we point a *different* harness at `openclaw mcp serve` and treat
  OpenClaw as just another MCP server. Both are valid adapter shapes.
- Provider/model config is per-harness (a-coder-cli `models.json`, Claude
  `--model` / `settings.json`, Codex per-thread `model`, …). The adapter
  writes the right config for its harness from the studio's single Navya/Local
  settings — the user configures Navya Cloud + local llama.cpp **once**, and
  each adapter materializes it in its harness's native format.

#### Why a-coder-cli is the reference impl
It has the richest, most bidirectional contract (persistent session,
`steer` / `follow_up` / `abort`, `set_model`, tool-execution events, and an
extension-UI dialog sub-protocol that maps 1:1 to Tauri dialogs). The others
are subsets of this surface, so building `AaaCoderCliHarness` first pins the
fullest `trait Harness` and forces every other adapter to declare its
degradations explicitly. The a-coder-cli SDK path (`docs/sdk.md`) remains
available if we later run the agent in a Node sidecar we control end-to-end.

> The harness layer is an **abstraction, not a dependency**. `trait Harness`
> in Rust + one impl today + adapters for the rest, all sharing one MCP tool
> server. A studio project opened in any of these harnesses is also a valid
> native project for that harness.

### 2.2 Navya Cloud + local LLM are just "providers" to the harness
Every harness in the matrix has a model/provider config mechanism, and all of
them treat an OpenAI-compatible endpoint as a first-class provider. The
studio's single settings (Navya endpoint + key, local llama.cpp URL) are
materialized by the active adapter into its harness's native format:

- **Navya Cloud** → an OpenAI-compatible provider entry pointing at the user's
  Navya `baseUrl`, authed by the user's Navya API key (OS keyring, injected as
  a runtime key). Navya already speaks OpenAI: `/v1/chat/completions`,
  `/v1/images/generations`, `/v1/videos/generations`, `/v1/audio`,
  `/v1/embeddings`, plus `navya/auto` routing (see `provider-api/backend/
  routers/media.py` and `main.py`).
- **Local LLM** → a llama.cpp server (OpenAI-compatible `/v1/chat/completions`)
  as another provider entry, `baseUrl http://localhost:PORT`.

The user picks "Cloud" or "Local" (or a specific model) in the UI; the Rust
core sends a normalized `set_model` command and the adapter translates it to
  the harness's native model selection (a-coder-cli `set_model`, Claude
  `--model`, Codex per-thread `model`, …). No studio-specific model code.

### 2.3 Local image gen = stable-diffusion.cpp `sd-server`
`stable-diffusion.cpp` ships both `sd-cli` (one-shot txt2img) **and** an
`sd-server` with an HTTP API (`docs/server`, `examples/server`). We bundle
`sd-server` as a Tauri sidecar (`tauri-plugin-shell`, `externalBin`), start it
lazily on first local image request, and call its HTTP endpoint from a Rust
command. This keeps generation streaming/progress visible in the UI without a
process-per-image cost. Build flavors per machine: `-DSD_CUDA=ON` (NVIDIA),
`-GGML_VULKAN=ON`, CPU fallback — the installer ships the matching binary.

Cloud image gen, by contrast, is just a tool that POSTs to Navya
`/v1/images/generations`.

### 2.4 Video = HyperFrames authored by the agent, rendered by a Node sidecar
HyperFrames renders video from HTML compositions via a Node 22 + FFmpeg CLI
(`npx hyperframes …`, see `hyperframes-cli` SKILL). Under the hood it is
Remotion: `@remotion/bundler` `bundle()` → `@remotion/renderer`
`selectComposition()` + `renderMedia()` / `renderStill()`, with Chrome
headless-shell and FFmpeg auto-fetched on first render.

The harness already owns the HyperFrames skills and can scaffold/author a
project (`npx hyperframes init`, `lint`, `check`, `preview --background`).
Renders are long and crash-prone, so they run in a **dedicated Node render
sidecar** rather than inside the harness process:

- The harness writes/edits the composition files (it has `read`/`write`/
  `edit`/`bash` tools).
- When the agent decides to render, a custom **Navya tool** (registered on the
  harness) hands the render job to the Rust core, which forwards it to the
  render sidecar. Progress streams back to the UI as render-queue events.
- For heavy/cloud renders HyperFrames also offers `lambda`, `cloudrun`, and
  HeyGen-hosted `cloud render` — the studio surfaces those as render targets.

### 2.5 Tauri 2 shell + plugins
From the context7 pass on Tauri 2.9 / plugins-workspace:

- `#[tauri::command]` + JS `invoke()` for the UI ↔ Rust IPC.
- `tauri-plugin-shell` to spawn and supervise sidecars (`Command::sidecar`),
  with scoped permissions in `tauri.conf.json` capabilities.
- `externalBin` in `tauri.conf.json` to bundle `a-coder-cli`, the Node
  runtime, and `sd-server`/`sd-cli` per-target triple. Platform suffixes
  (`-x86_64-pc-windows-msvc.exe`) follow Tauri's sidecar convention.
- `tauri-plugin-sql` (SQLite) for project/asset/render history, and
  `tauri-plugin-store` or the OS keyring for the Navya key + local model paths.

---

## 3. Project layout

```
navya-studio/
├── concept.md                  # the brief
├── ARCHITECTURE.md             # this file
├── RESEARCH.md                 # raw context7 findings
├── src-tauri/                  # Rust core (the desktop binary)
│   ├── Cargo.toml
│   ├── tauri.conf.json         # sidecars (externalBin), capabilities, CSP
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs              # tauri::Builder, plugin registration, commands
│   │   ├── commands/          # #[tauri::command] handlers (ui → rust)
│   │   ├── sidecar/            # spawn/supervise harness, render, sd
│   │   ├── harness/           # trait Harness + adapters:
│   │   │                       #   AaaCoderCliHarness (RPC), ClaudeCodeHarness,
│   │   │                       #   CodexHarness (app-server), AntigravityHarness,
│   │   │                       #   HermesHarness (JSON-RPC), OpenClawHarness (WS)
│   │   ├── control/            # loopback HTTP+WS control server = tool LOGIC
│   │   ├── tools/              # #[tauri::command] wrappers over the same logic
│   │   ├── navya/              # Navya Cloud HTTP client (chat/images/video)
│   │   ├── sd/                 # sd-server HTTP client + lifecycle
│   │   ├── render/             # render-queue, forwards to Node sidecar
│   │   ├── store/              # SQLite models + settings/secrets
│   │   └── events.rs           # emit Tauri events to the webview
│   └── binaries/               # bundled sidecar binaries (git-lfs or build step)
├── navya-mcp/                 # FastMCP (Python) MCP server: stdio binding,
│                              #   proxies each tool to src-tauri/control
│   pyproject.toml requirements.txt (fastmcp, httpx)
│   navya_mcp/server.py        # @mcp.tool for each studio tool
├── ui/                         # React + Vite + Tailwind (the webview SPA)
│   ├── package.json
│   ├── src/
│   │   ├── App.tsx
│   │   ├── chat/               # agent conversation (streams harness events)
│   │   ├── timeline/           # HyperFrames preview + edit
│   │   ├── renders/           # render queue + progress
│   │   ├── models/             # Cloud/Local picker, model list
│   │   ├── harness/            # harness picker (a-coder-cli / claude / codex / …)
│   │   └── lib/                # invoke() wrappers, event listeners
└── harness-pack/              # per-harness config the adapter materializes
    ├── a-coder-cli/           # models.json, skills/, prompts/, extensions/
    ├── claude-code/           # settings.json (permissions, mcp), hooks/
    ├── codex/                 # app-server thread config, dynamicTools schemas
    ├── antigravity/           # ~/.claude/mcp-servers.json fragment
    ├── hermes/                # mcp.servers yaml, plugin hooks
    └── openclaw/              # gateway token + mcp serve args
```

`navya-mcp/` (FastMCP) is the MCP binding for the five MCP-capable harnesses;
`src-tauri/control/` is the single source of tool logic. `harness-pack/<harness>/`
is the small per-harness config the adapter writes (provider entries,
permissions, hooks, MCP wiring: e.g. the `mcpServers` JSON pointing at
`uv run --with fastmcp ... navya_mcp/server.py` with `NAVYA_CONTROL_*` env).
A project opened in the studio is also a valid native project for whichever
harness is selected.



---

## 4. The request loop (one concrete run)

User types "make a 30s faceless explainer about black holes" and picks
Navya Cloud.

1. UI calls Rust `send_prompt(text)`.
2. Rust `HarnessAdapter` (the selected harness's adapter; a-coder-cli by
   default) spawns the harness with `cwd = <project dir>`, injects our MCP
   tool server into the harness's MCP config, and materializes the harness's
   native model/permission config from the user's Navya/Local settings. It
   then sends the normalized `prompt` command in the harness's native protocol.
3. The harness loads its skills — `/hyperframes` routes to the faceless-
   explainer workflow, which runs the intent interview and writes `BRIEF.md`.
4. Agent calls its tools (all visible to the UI via `tool_execution_*`
   events):
   - `bash`/`write`/`edit` to scaffold the HyperFrames project and author HTML.
   - `generate_image` (an MCP tool) → Rust picks Cloud or Local. Cloud:
     POST Navya `/v1/images/generations`; Local: ensure `sd-server` sidecar is
     up, POST to it. Image saved into the project assets dir.
   - `npx hyperframes lint` / `check` / `preview --background` via `bash`.
5. Agent calls `render_to_video` (an MCP tool) → Rust enqueues a render
   in the Node sidecar (`renderMedia` / `npx hyperframes render --quality high`).
   Progress emits Tauri events the `renders/` panel subscribes to.
6. On completion Rust writes a render record to SQLite and surfaces the file
   in the UI with a "reveal in folder" action.

Throughout, the UI is just rendering the harness event stream
(`message_update` deltas, `tool_execution_*`, `queue_update`) — exactly the
protocol in `docs/rpc.md`.

---

## 5. Cloud vs Local — one switch, two paths

| | Cloud | Local |
|---|---|---|
| LLM | Navya `/v1/chat/completions` (incl. `navya/auto`) | llama.cpp server, OpenAI-compatible |
| Images | Navya `/v1/images/generations` | `sd-server` sidecar (CUDA/Vulkan/CPU) |
| Video gen (optional) | Navya `/v1/videos/generations` | not supported locally |
| Video render | local HyperFrames/Remotion sidecar (same for both) | same |
| Cost | billed by Navya | free, user's GPU/CPU |

The studio does not hard-code either path. The `generate_image` and
`set_generation_source` tools, plus the model picker, route at runtime.

---

## 6. Open questions to resolve before scaffolding

1. **Node sidecar packaging** — ship a bundled Node 22 (e.g. via
   `pkg`/`bun`/standalone Node binary) or require Node on the host? Bundling is
   better UX but heavier; HyperFrames' `render --docker` is a fallback that
   avoids shipping Node at all.
2. **Sidecar binary distribution** — `sd-server` and a Node runtime are large
   and platform/build-flavor specific. Decide: per-target prebuilt downloads
   on first run vs. bundling in the installer (impacts Tauri bundle size).
3. **Which harnesses ship first?** a-coder-cli is the reference impl; the
   MCP tool server means a second harness (claude-code or codex) is cheap to
   add since tools are shared. Decide the launch set and whether OpenClaw's
   WebSocket adapter is in v1 or deferred.
4. **Navya Cloud endpoint + auth flow** — confirm base URL, whether the studio
   embeds Navya API key entry / OAuth, and whether BYOK to underlying providers
   (the `byok_router` exists in Navya) is exposed.
5. **Project model** — **resolved (kick-off): a studio project holds many compositions.** SQLite has a `compositions` table; the left rail has a composition switcher (promoted into v1 from design.md §12); the timeline/preview and renders are per-composition.

---

## 7. What to build first (milestone 0)

A vertical slice that proves the loop without the hard packaging:

1. `cargo create-tauri-app` (Rust + React), add `tauri-plugin-shell`.
2. `trait Harness` + `AaaCoderCliHarness` RPC adapter (spawn, prompt,
   steer, abort, stream events). Build the **MCP tool server** in Rust with one
   tool (`generate_image`, cloud path only) so the tool path is proven
   harness-agnostic from day one. Hard-code `a-coder-cli` from PATH; assume
   Node + hyperframes installed on the dev machine.
3. Navya Cloud registered as a provider in the adapter's `models.json`; key
   from env.
4. Add a **second harness adapter stub** (claude-code print/stream mode) that
   reuses the same MCP tool server — this is the cheap proof that the
   harness-agnostic design actually holds, not just on paper.
5. UI: a harness picker + a chat box that streams the normalized event stream
   + a "renders" pane.
6. End-to-end on both harnesses: prompt → agent scaffolds a HyperFrames
   project → renders via `bash npx hyperframes render` → video appears.

Local SD.cpp, render sidecar isolation, keyring, the render queue, and
multi-harness support come after the slice is green.