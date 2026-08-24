# Navya Studio — Research Notes (context7)

Raw findings behind `ARCHITECTURE.md`. Each section names the source on
context7 so it can be re-pulled.

---

## Navya Cloud (read from `provider-api/`, not context7)

OpenAI-compatible FastAPI proxy at `C:\Users\hamis\Downloads\code\a-coder\provider-api`.
Routers (from `backend/main.py`):

- `auth_router`, `oauth_router`, `admin_router`, `proxy_router`
- `chat_router` → `/v1/chat/completions` (OpenAI shape)
- `models_router` → dual-format models listing
- `usage_router`, `audio_router`, `embeddings_router`, `billing_router`
- `media_router` → **`POST /v1/images/generations`**, **`POST /v1/videos/generations`**
  (from `backend/routers/media.py`): proxies to active providers of kind
  `image`/`video`, logs synthetic-token usage, deducts balance. OpenAI request
  & response shapes pass through unchanged.
- `referral_router`, `byok_router`, `tracing_router`, `training_router`,
  `engine_pairing_router`, `provider_payout_router`, `mcp_router`,
  `skills_router`.

Notable: `navya/auto` model = a master LLM (default `Qwen/Qwen3-32B-TEE`)
routes to the best active model per request (`backend/services/router.py`,
`AUTO_ROUTER_*` env). Auth via JWT/api keys, SQLite (aiosqlite), Alembic.

**Implication for Navya Studio:** the studio can treat Navya as one
OpenAI-compatible provider covering chat, images, video, audio, embeddings —
no bespoke client per modality.

---

## Tauri 2.9 (context7: `/websites/rs_tauri_2_9_3`, `/tauri-apps/plugins-workspace`)

- `tauri::Builder::default().plugin(...).run(tauri::generate_context!())`.
- IPC: `#[tauri::command] async fn handler(...)` + `generate_handler![...]`;
  frontend `invoke('handler', {...})`. Custom invoke uses `fetch('my-impl://command', ...)`.
- `tauri.conf.json`: `build.devUrl`, `build.frontendDist`, `app.security.csp`,
  `app.windows[]`, `bundle`, `plugins`.
- Plugins are built with `PluginBuilder::new("name").invoke_handler(...).build()`
  and registered via `.plugin(plugin::init())`.

### tauri-plugin-shell (plugins-workspace `/v2/plugins/shell`)
- Cargo: `tauri-plugin-shell = "2.0.0"`. JS: `import { Command } from '@tauri-apps/plugin-shell'`, `Command.create('git', [...])`.
- Register `tauri_plugin_shell::init()` in `main`.
- Spawns child processes / sidecars, opens URLs. Cross-platform.
- Sidecars: declare in `tauri.conf.json` `bundle.externalBin` with per-target
  suffixes; spawn via `Command::new_sidecar(...)`. Scoped via capabilities.

**Implication:** all three sidecars (harness, Node render, sd-server) are
spawned/supervised from Rust via the shell plugin.

---

## Remotion (context7: `/remotion-dev/remotion`)

- Server-side render path:
  ```ts
  import { bundle } from '@remotion/bundler';
  import { selectComposition, renderMedia, renderStill, ensureBrowser } from '@remotion/renderer';
  const serveUrl = await bundle({ entryPoint: './src/index.ts', webpackOverride });
  const composition = await selectComposition({ serveUrl, id, inputProps });
  await renderMedia({ composition, serveUrl, codec: 'h264', outputLocation, inputProps });
  // still: renderStill({ composition, serveUrl, output, inputProps });
  ```
- `ensureBrowser({ onBrowserDownload })` downloads Chrome headless-shell to
  `node_modules/.remotion/chrome-headless-shell/[platform]/...`. Can pass
  `browserExecutable` to use a system browser.
- `npx remotion install ffmpeg` / `install ffprobe` — or auto-fetched on
  first render.
- Input props are passed to both `selectComposition` and `renderMedia`.
- Batch/dataset rendering: one `bundle()` then loop `selectComposition` +
  `renderMedia` per row.

**Implication:** a Node sidecar wrapping `bundle` + `renderMedia` is the
studio's render engine. Chrome + FFmpeg are fetched on demand; we can also
ship them bundled for offline installs.

---

## HyperFrames (read from skill `~/.agents/skills/hyperframes*/SKILL.md`)

- `npx hyperframes ...`. Requires **Node 22+** and **FFmpeg**.
- Lifecycle: `init <project>` → `catalog --query` → `add <name>` → author
  HTML → `lint` → `check` → `preview --background` → `render`.
- Render targets:
  - `render --quality draft|high --output out.mp4` (local)
  - `render --docker --strict` (containerised)
  - `render --batch rows.json --output "renders/{name}.mp4"`
  - `cloud render` (HeyGen-hosted), `lambda render`, `cloudrun render`
- `--json` for agent/CI calls; `doctor --json` gate (`jq -e '.ok'`).
- Workflows (routes): `/faceless-explainer`, `/product-launch-video`,
  `/motion-graphics`, `/general-video`, `/talking-head-recut`, etc. The intent
  interview writes `BRIEF.md`; the workflow reads only the brief.
- HyperFrames renders video from HTML compositions; timing via `data-*`
  attributes, seekable animation runtime (`hyperframes-core`). Underlying
  render engine is Remotion.

**Implication:** HyperFrames is the authoring layer the harness drives.
The studio's render sidecar can just run `npx hyperframes render` (simplest)
or call `@remotion/renderer` directly for tighter control/progress.

---

## stable-diffusion.cpp (context7: `/leejet/stable-diffusion.cpp`)

- Two binaries:
  - `sd-cli` (one-shot txt2img):
    `./bin/sd-cli -m model.ckpt -p "prompt" -H 1024 -W 1024`
    Flux: `--diffusion-model flux1-dev-q3_k.gguf --vae ae.sft --clip_l ... --t5xxl ...`
    SD3: `-m sd3_medium... --clip-on-cpu --cfg-scale 4.5 --sampling-method euler`
  - `sd-server` (HTTP server, `examples/server`):
    `sd-server.exe --diffusion-model ... --vae ... --llm ... --diffusion-fa --offload-to-cpu -v --cfg-scale 1.0`
- Build:
  - CUDA: `cmake .. -DSD_CUDA=ON` (needs ≥4 GB VRAM)
  - Vulkan: `cmake .. -DGGML_VULKAN=ON`
  - RPC server (distributed): `-DGGML_RPC=ON -DGGML_VULKAN=ON ...`
- Supports `.ckpt`, `.safetensors`, `.gguf`; SD, SDXL, SD3, Flux.

**Implication:** bundle `sd-server` as a Tauri sidecar, start on first local
image request, talk HTTP from Rust. One process, streaming/progress, and it
works for SD/SDXL/SD3/Flux by swapping model files the user points at.

---

## a-coder-cli harness (read from `~/.a-coder/lib/a-coder-cli/docs/`)

### SDK (`docs/sdk.md`, package `@earendil-works/pi-coding-agent`)
- `createAgentSession({ model, thinkingLevel, tools, customTools,
  resourceLoader, sessionManager, settingsManager, authStorage, modelRegistry,
  cwd, agentDir })` → `{ session }`.
- `session.prompt(text, { images, streamingBehavior })`, `.steer()`,
  `.followUp()`, `.abort()`, `.subscribe(listener)`, `.setModel(...)`,
  `.compact()`, `.dispose()`.
- Custom tools: `defineTool({ name, parameters: Type.Object(...), execute })`
  passed via `customTools`.
- Skills, prompt templates, context files, extensions injectable via
  `DefaultResourceLoader` overrides.
- `AuthStorage` (runtime keys, `auth.json`, env fallback), `ModelRegistry`
  (`models.json` custom providers), `SessionManager` (in-memory or file tree).
- Run modes: `InteractiveMode`, `runPrintMode`, **`runRpcMode`**.

### RPC mode (`docs/rpc.md`, `a-coder-cli --mode rpc --no-session`)
- Strict JSONL over stdio (LF only; do **not** use Node `readline` — it splits
  on U+2028/U+2029).
- Commands: `prompt`, `steer`, `follow_up`, `abort`, `new_session`,
  `get_state`, `get_messages`, `set_model`, `cycle_model`,
  `get_available_models`, `set_thinking_level`, `compact`,
  `set_auto_compaction`, `bash`, `get_session_stats`, `switch_session`,
  `fork`, `clone`, `get_entries` (cursor via `since`), `get_tree`,
  `get_last_assistant_text`, `set_session_name`, `get_commands`.
- Events: `agent_start/end`, `turn_start/end`, `message_start/update/end`,
  `tool_execution_start/update/end`, `queue_update`, `compaction_*`,
  `auto_retry_*`, `extension_error`.
- Extension UI sub-protocol: `extension_ui_request` (select/confirm/input/
  editor/notify/setStatus/setWidget/setTitle/set_editor_text) ↔
  `extension_ui_response` — lets the harness ask the UI questions.
- Python client example in the docs spawns the CLI and pipes JSON.

**Implication:** RPC mode is the language-agnostic contract Rust drives.
The extension-UI sub-protocol is exactly how the harness asks the user
confirmations that surface as native Tauri dialogs. The SDK path is available
later if we move the harness in-process (e.g. if we ever run the agent in a
Node sidecar we control end-to-end).

### Models / custom providers (`docs/custom-provider.md`, `docs/models.md`)
- `models.json` under `agentDir` registers OpenAI-compatible providers with
  `baseUrl`, api id, key reference. The harness then treats them as native.

**Implication:** Navya Cloud and a local llama.cpp server are each one
`models.json` entry — no studio-side model plumbing.

---

## Harness-agnostic claim (from concept.md)
> harness: a-coder-cli … but can be harness agnostic. We'll support, claude
> code, hermes agent, openclaw, openai codex, antigravity cli.

Researched each on context7. Two structural findings drive the design:

1. **MCP covers five of six — a-coder-cli is the exception.** Claude Code,
   Codex, Antigravity, Hermes, and OpenClaw all support MCP servers for custom
   tools. a-coder-cli **intentionally has no built-in MCP** (`docs/usage.md`:
   "It intentionally does not include built-in MCP … You can build or install
   those workflows as extensions"); it uses TypeScript extensions +
   `customTools`/`pi.registerTool` instead. So the studio tools are a **single
   Rust backend** exposed two ways: (a) a stdio **MCP server** (`navya-mcp`)
   for the five MCP harnesses, and (b) a thin **a-coder-cli TypeScript
   extension** that proxies the same tool calls to the backend. The tool
   *logic* is written once; only the thin transport shim differs per family.

### Claude Code (context7 `/websites/code_claude`, `/anthropics/claude-code`)
- Print/stream: `claude -p --output-format stream-json --verbose
  --include-partial-messages [--include-hook-events]` (NDJSON: init,
  assistant text deltas, tool_use, tool_result, final result).
- Bidirectional persistent session: `--input-format stream-json` streams
  user messages + control messages into one session; `--resume`,
  `--continue`, `--session-id` for continuity.
- SDKs: TS `@anthropic-ai/claude-agent-sdk` and Python `claude_agent_sdk`
  (`ClaudeSDKClient.query(stream)` + `receive_response()` + follow-up in same
  session).
- Approvals/permissions: `settings.json` `permissions.allow/deny`; hooks
  `PreToolUse` / `PermissionRequest` return `allow`/`deny` (+ `interrupt`).
- MCP: `mcpServers` config + `allowedTools` (e.g. `mcp__github__*`).
- Model: `--model <id>` override.

### OpenAI Codex (context7 `/openai/codex`)
- One-shot headless: `codex exec --json` emits `ThreadEvent` JSONL:
  `thread.started`, `turn.started/completed/failed`, `item.started/updated/
  completed`, `error`, with `Usage` (input/output/cache tokens). Defaults
  `approval_policy: Never`.
- **Persistent JSON-RPC: `codex app-server`** with `thread/start` (returns
  thread id), `thread/started` notification, `command/exec`, `dynamicTools`
  (custom function tool namespaces w/ JSON schema), `approvalPolicy`,
  `sandbox`/`permissions`, `personality`.
- MCP supported.
- Model: per-thread `model` param (e.g. `gpt-5.1-codex`).

### Antigravity CLI `agy` (context7 `/google-antigravity/antigravity-cli`)
- Print mode: `agy -p "<prompt>" --output-format text|json|stream-json`.
  stream-json = NDJSON: init events, `step_update` (thinking/tool use), final
  result. `--json-schema` validates `json` output.
- Tools: MCP via `~/.claude/mcp-servers.json` (Claude-Code-compatible file).
- Looks primarily **one-shot per prompt** (no documented persistent stdin
  stream). Each turn = a process (or session the adapter manages).

### Hermes Agent (context7 `/nousresearch/hermes-agent`)
- **Bidirectional stdio JSON-RPC gateway**: `tui_gateway/entry.py` reads
  newline-delimited JSON from stdin, dispatches, writes JSON-RPC responses;
  emits `gateway.ready` first. Real persistent RPC sidecar.
- MCP native: `ctx.call_mcp(server, tool, args, timeout)` from plugins/tools,
  stdio server recycling, trust tiers. `mcporter call … --output json`.
- Python plugin/tool system (custom tools, hooks). Persistent memory.
- There's already a `hermes-desktop` app (reference for our Tauri shell).

### OpenClaw (context7 `/openclaw/openclaw`)
- **Gateway-centric, not a pipe-able CLI**: runs a WebSocket Gateway
  (`ws://127.0.0.1:18789`); SDK `new OpenClaw({ url, token })`, JSON-RPC
  `tools.invoke` with `confirm: "request" | "report"` approval modes
  (returns `requiresApproval: true` on block).
- MCP bridge: `openclaw mcp serve --url … --token-file …` exposes the gateway
  to an MCP client over stdio. So OpenClaw joins the studio as an MCP server
  we point a harness at, OR via its own SDK from Rust (WS).
- Different shape from the others — the adapter connects over WS, not stdio.

### Capability matrix (what each adapter can promise)

| Harness          | Transport         | Persistent session | Steer mid-stream | Abort | Custom tools   | Approvals → UI         |
|------------------|-------------------|--------------------|-----------------|-------|----------------|------------------------|
| a-coder-cli      | stdio JSONL (RPC) | yes                | yes (`steer`)    | yes   | ext/customTools (no MCP) | extension-UI dialogs   |
| Claude Code      | stdio stream-json | yes (`--resume`)    | yes (input stream+interrupt) | yes | MCP + hooks | `PreToolUse`/`PermissionRequest` |
| Codex            | app-server JSON-RPC | yes (threads)    | yes (thread)     | yes   | MCP + `dynamicTools` | `approvalPolicy` |
| Antigravity `agy`| stdio stream-json | per-prompt (one-shot) | no (→ abort+re-prompt) | kill proc | MCP only | MCP / permissions |
| Hermes           | stdio JSON-RPC    | yes                | yes              | yes   | MCP + Python   | plugin hooks           |
| OpenClaw         | WebSocket gateway | yes (gateway)     | yes              | yes   | MCP + `tools.invoke` | `confirm: request/report` |

### Design consequences
- `trait Harness` in Rust is a **protocol translator**, not a shared wire
  format. Each adapter (a) spawns/connects the harness, (b) injects our MCP
  server into the harness's MCP config, (c) translates Navya commands
  (`prompt`/`steer`/`abort`/`set_model`) to the native protocol, (d) emits a
  normalized `HarnessEvent` stream the UI renders, (e) routes approvals to the
  Tauri UI.
- **Studio tools ship as one FastMCP (Python) server** (`navya-mcp/`) — the
  single MCP binding shared by five MCP-capable harnesses; a-coder-cli uses
  a TS extension to the same Rust control-server backend. Tool logic lives
  once in Rust; FastMCP and the TS extension are both thin proxies.
- Capabilities degrade explicitly per adapter (matrix above): a harness
  without mid-stream steer implements `steer` as `abort` + re-prompt; a
  print-mode harness (Antigravity) runs one process per turn; OpenClaw's
  adapter speaks WS instead of stdio.
- Provider/model config is per-harness (a-coder-cli `models.json`, Claude
  `--model`/settings, Codex thread `model`, …). The adapter writes the right
  config for its harness from the studio's single Navya/Local settings.
---

## FastMCP — the MCP server framework (context7 `/prefecthq/fastmcp`)

We build `navya-mcp` (the studio's MCP tool server, consumed by the five
MCP-capable harnesses) with **FastMCP**, a Python MCP framework. `navya-mcp`
is a **thin proxy**: it exposes the studio tools over stdio MCP and forwards
each call to the Rust core's loopback control server (Phase 3). Tool *logic*
stays in Rust; FastMCP gives us spec-correct MCP framing, `tools/list`, and
per-call progress/logging for free, instead of hand-rolling the MCP stdio
protocol in Rust.

### Core API
```python
from fastmcp import FastMCP, Context

mcp = FastMCP("navya-studio")

@mcp.tool
async def generate_image(prompt: str, width: int = 1024, height: int = 1024,
                         ctx: Context) -> dict:
    await ctx.info(f"generating: {prompt}")
    await ctx.report_progress(0, 100, "queued")
    # proxy to the Rust control server
    async with httpx.AsyncClient(timeout=300) as http:
        r = await http.post(f"{CONTROL_URL}/tool/generate_image",
            headers={"Authorization": f"Bearer {CONTROL_TOKEN}"},
            json={"prompt": prompt, "width": width, "height": height})
    await ctx.report_progress(100, 100, "done")
    return r.json()   # { path, source, cost_usd, ... }

if __name__ == "__main__":
    mcp.run()    # STDIO transport by default
```
- `@mcp.tool` (or `@server.tool`) registers a tool; parameters are plain typed
  Python args → auto JSON-schema (Pydantic). Async supported.
- `ctx: Context` → `ctx.info/debug/warning/error`, `ctx.report_progress(done,
  total, message)`, `ctx.read_resource(...)`, `ctx.set_state/get_state`,
  `ctx.transport` (`"stdio"` | `"sse"` | `"streamable-http"`), `ctx.request_id`.
  Progress streams back to the harness client (and thus the UI tool card).
- `mcp.run()` = **STDIO by default**. Also `mcp.run(transport="http",
  host=, port=)` / SSE / Streamable HTTP.
- Image results: FastMCP's `Image` content type (return an `Image(path=…)` or
  bytes) so a harness renders the thumbnail in-chat; for file assets we usually
  return a path + metadata and let the studio UI show the thumbnail.

### CLI & harness wiring
- Run: `fastmcp run server.py` (or `python server.py`). Manage deps via uv:
  `fastmcp run server.py --with httpx --with-requirements requirements.txt`.
- Install into a harness: `fastmcp install claude-code server.py`,
  `fastmcp install claude-desktop server.py --with-requirements reqs.txt`.
- Harness `mcpServers` config (Claude Code / Cursor / Antigravity shape):
  ```json
  { "mcpServers": { "navya-studio": {
      "command": "uv", "args": ["run","--with","fastmcp","--with","httpx",
        "fastmcp","run","<abs path>/navya_mcp/server.py"],
      "env": { "NAVYA_CONTROL_URL": "http://127.0.0.1:PORT",
               "NAVYA_CONTROL_TOKEN": "<from keyring>" } } } }
  ```
  The harness spawns the server as a child; `NAVYA_CONTROL_*` env is how the
  proxy reaches the Rust core (port + token written to the keyring by the core
  and injected per-harness by the adapter).

### Implications
- **Python in the stack.** `navya-mcp/` is a Python project (`pyproject.toml`,
  `requirements.txt`: `fastmcp`, `httpx`). Bundling (Phase 14): use `uv run
  --with fastmcp …` (downloads on first run) or bundle a standalone Python +
  the deps into the installer. `uv` is the recommended runner.
- **One logic, two bindings, unchanged.** The Rust control server is still the
  single source of tool logic; the FastMCP server (MCP harnesses) and the
  a-coder-cli TS extension (reference harness) both proxy to it. We do **not**
  reimplement tool logic in Python.
- **a-coder-cli still doesn't use this.** a-coder-cli has no MCP client, so it
  keeps the TypeScript extension (Phase 5). FastMCP only serves the five
  MCP-capable harnesses (Phase 12 minimal, Phase 13 full).
- **Tests:** FastMCP ships `fastmcp.utilities.tests.run_server_in_process` for
  subprocess-isolated stdio tests; use it for a conformance test that the five
  harness configs can `list_tools` and `call_tool` against `navya-mcp`.
