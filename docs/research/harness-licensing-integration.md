# Navya Studio — Harness Integration Details and Licensing

Research report for the six harness adapters. Based on context7 passes of
a-coder-cli, Claude Code, OpenAI Codex, Antigravity, Hermes Agent, and
OpenClaw, plus the local a-coder-cli binary on the build machine.

## Executive summary

- **a-coder-cli** is the reference harness and is installed as `pi.exe` on the
  build machine; the binary name in PATH is `a-coder-cli` / `pi`.
- **Claude Code**, **Codex**, **Hermes**, and **OpenClaw** are user-installed in
  v1; bundling them requires license review (Anthropic, OpenAI, Nous Research,
  OpenClaw Foundation).
- **Hermes** is MIT-licensed and could potentially be bundled after legal
  review.
- **Antigravity** is Google; integration details are sparse — assume
  Claude-Code-compatible MCP config and one-shot print mode.
- All five MCP-capable harnesses wire the same `navya-mcp` FastMCP server via
their native MCP config format.

## a-coder-cli (reference)

### Binary / invocation

On the build machine the executable is located at:

```text
C:\Users\hamis\.a-coder\lib\a-coder-cli\pi.exe
```

The CLI reports itself as `a-coder-cli`. Relevant options from `--help`:

| Flag | Meaning |
|---|---|
| `--provider <name>` | Provider name (default `google`) |
| `--model <pattern>` | Model pattern or ID (`provider/id` or `id:<thinking>`) |
| `--api-key <key>` | API key (env fallback) |
| `--system-prompt <text>` | System prompt |
| `--append-system-prompt <text>` | Append text/file to system prompt |
| `--mode <mode>` | `text` (default), `json`, or `rpc` |
| `--permission-mode <mode>` | `ask`, `allow`, `read-only`, `auto` |
| `--print, -p` | Non-interactive mode |

The RPC mode command is:

```bash
a-coder-cli --mode rpc --no-session --provider <provider> --model <model>
```

(Exact `--no-session` flag must be verified against the installed version; see
`docs/rpc.md`.)

### Custom tools

a-coder-cli has **no built-in MCP** and uses TypeScript extensions with
`pi.registerTool`. Navya Studio ships an extension at
`harness-pack/a-coder-cli/extensions/navya-studio.ts` that POSTs to the Rust
control server.

### Model/provider config

`models.json` under the agent directory registers OpenAI-compatible providers.
Key fields from `docs/custom-provider.md` / `docs/models.md`:

```json
{
  "providers": [
    {
      "id": "navya-cloud",
      "name": "Navya Cloud",
      "baseUrl": "https://api.navya.example/v1",
      "api": { "keyRef": "navya-api-key" },
      "models": [
        { "id": "navya/auto", "name": "Navya Auto" }
      ]
    }
  ]
}
```

Runtime keys are injected via the a-coder-cli `AuthStorage` mechanism
(`auth.json` / env fallback) — **do not write secrets into `models.json`**.

### Approval / dialog sub-protocol

The RPC stream emits `extension_ui_request` events:

```json
{
  "type": "extension_ui_request",
  "subtype": "select" | "confirm" | "input" | "editor" | "notify" | "setStatus" | "setWidget" | "setTitle" | "set_editor_text",
  "id": "req-1",
  "payload": { ... }
}
```

The studio answers with `extension_ui_response`:

```json
{
  "type": "extension_ui_response",
  "id": "req-1",
  "value": "...",
  "confirmed": true
}
```

### Licensing

a-coder-cli is the user's own `pi-mono` install. It is not redistributed by the
studio in v1; the studio simply spawns the installed binary.

## Claude Code

### Binary / invocation

Binary: `claude` (install via Anthropic's official installer). Persistent
bidirectional session:

```bash
claude -p --output-format stream-json --verbose --include-partial-messages \
  --input-format stream-json --model <model> --resume --session-id <sid>
```

Flags:

| Flag | Meaning |
|---|---|
| `-p, --print` | Headless / non-interactive |
| `--output-format stream-json` | NDJSON events out |
| `--input-format stream-json` | Streaming input in |
| `--verbose` | More events |
| `--include-partial-messages` | Text deltas |
| `--include-hook-events` | Hook events (PreToolUse, etc.) |
| `--model <id>` | Override model |
| `--session-id <id>` / `--resume` | Session continuity |
| `--append-system-prompt <text>` | Inject system prompt |
| `--dangerously-skip-permissions` | Skip all permission prompts |

### Steer / abort

Send a control/interrupt message on the streaming input channel:

```json
{ "type": "interrupt" }
```

A user message on the input stream acts as a steer/follow-up.

### Custom tools

MCP via `~/.claude/settings.json` `mcpServers`:

```json
{
  "mcpServers": {
    "navya-studio": {
      "command": "uv",
      "args": ["run", "--with", "fastmcp", "--with", "httpx", "fastmcp", "run", "<abs-path>/navya_mcp/server.py"],
      "env": {
        "NAVYA_CONTROL_URL": "http://127.0.0.1:<port>",
        "NAVYA_CONTROL_TOKEN": "<token>"
      }
    }
  }
}
```

Studio adapter writes `.claude/settings.json` into the project dir (or the
user's Claude Code config dir).

### Approvals

- `PreToolUse` hook: returns `allow`, `deny`, or `interrupt`.
- `PermissionRequest` hook: same.
- Allow rules can be written into `settings.json` `permissions.allow`.
- Deny broad rules like `Bash(*)` and reads of `.env` / keyring dirs.

### Licensing

Claude Code is Anthropic proprietary software. **Not redistributable**. Studio v1
requires the user to install Claude Code themselves.

## OpenAI Codex

### Binary / invocation

Binary: `codex` (install via OpenAI). Two modes:

1. **One-shot headless:** `codex exec --json`
2. **Persistent JSON-RPC:** `codex app-server`

Studio uses the app-server. Example startup (from test client docs):

```bash
codex app-server --listen ws://127.0.0.1:4222
```

Default listen is `stdio://`. Supported transports: `stdio://`, `unix://`,
`unix://PATH`, `ws://IP:PORT`, `off`.

### Initialization

Before any other RPC, the client must send `initialize` and then an `initialized`
notification.

```json
{ "method": "initialize", "id": 1, "params": { "clientInfo": { "name": "navya-studio", "version": "0.1.0" }, "capabilities": { "experimentalApi": true } } }
```

### Key JSON-RPC methods

| Method | Purpose |
|---|---|
| `initialize` / `initialized` | Handshake |
| `thread/start` | Create/resume a thread |
| `turn/start` | Add user input + begin generation |
| `turn/interrupt` | Cancel a turn |
| `command/exec` | Send a command/user message |
| `dynamicTools/register` | Register custom tool schemas |
| `model-list` | List models |

### Events

The server streams notifications:

- `thread/started`
- `turn/started`, `turn/completed`, `turn/failed`
- `item/started`, `item/updated`, `item/completed`
- `error`
- `approval/request`

A `turn/completed` with `status: "interrupted"` confirms abort.

### Model / provider config

Codex uses `~/.codex/config.toml` (or `CODEX_HOME`):

```toml
model = "codex"
model_provider = "openai"

[model_providers.navya]
base_url = "https://api.navya.example/v1"
env_key = "NAVYA_API_KEY"

[profiles.navya]
model_provider = "navya"
model = "navya/auto"
approval_policy = "on-request"
```

Runtime overrides via CLI:

```bash
codex -c model="navya/auto" -c model_provider="navya"
```

### Approval policy

Valid values (from `protocol.rs`):

- `untrusted` (alias `unless_trusted`)
- `on-request` (default)
- `granular`
- `never`

Studio should set `approval_policy = "on-request"` so Codex asks before
destructive commands.

### Custom tools

Codex supports MCP servers **and** `dynamicTools`. Either register Navya tools as
dynamic tool schemas, or point Codex at `navya-mcp` via MCP config.

### Licensing

OpenAI Codex is OpenAI proprietary software. **Not redistributable**. User-installed
in v1.

## Antigravity CLI (`agy`)

### Binary / invocation

Binary: `agy` (install via `https://antigravity.google/cli/install.sh`).
One-shot print mode:

```bash
agy -p "<prompt>" --output-format stream-json --model <model>
```

Flags:

| Flag | Meaning |
|---|---|
| `-p, --print` | Headless prompt |
| `--output-format text\|json\|stream-json` | Output format |
| `--json-schema <schema>` | Validate JSON output |
| `--model <id>` | Model override |

### Persistent session

No documented persistent stdin session. The adapter should cache the working
dir and re-spawn `agy -p ...` per turn. Set `Capabilities.steer = false` and
degrade `steer` to `abort` + re-prompt.

### Custom tools

Antigravity uses the **Claude-Code-compatible** `~/.claude/mcp-servers.json`
file:

```json
{
  "mcpServers": {
    "navya-studio": {
      "command": "uv",
      "args": ["run", "--with", "fastmcp", "--with", "httpx", "fastmcp", "run", "<abs-path>/navya_mcp/server.py"],
      "env": { "NAVYA_CONTROL_URL": "...", "NAVYA_CONTROL_TOKEN": "..." }
    }
  }
}
```

### Approvals

Antigravity config has `permission_mode` and `permission_grants`:

```json
{
  "permission_mode": "interactive",
  "permission_grants": [
    { "pattern": "git fetch && git rebase", "allow": true, "expiry": null }
  ]
}
```

The adapter should write scoped allow rules there.

### Licensing

Google Antigravity CLI. Terms unknown; assume **not redistributable**. Treat as
user-installed in v1.

## Hermes Agent

### Binary / invocation

Binary: `hermes` / `hermes-agent`. Headless gateway entry point:

```bash
python -m tui_gateway.entry
```

The gateway emits a `gateway.ready` JSON-RPC event first, then listens for
requests on stdin and writes responses to stdout.

### Key JSON-RPC methods

From `website/docs/developer-guide/programmatic-integration.md`:

```text
prompt.submit           prompt.background       session.steer
session.create          session.list            session.active_list
session.activate        session.close           session.interrupt
session.history         session.compress        session.branch
session.title           session.usage           session.status
clarify.respond         sudo.respond            secret.respond
approval.respond        config.set / config.get commands.catalog
command.resolve         command.dispatch        cli.exec
reload.mcp              reload.env              process.stop
delegation.status       subagent.interrupt      subagent.steer
spawn_tree.save / list / load
terminal.resize         clipboard.paste         image.attach
```

### Prompt / steer / abort

- `prompt.submit` — start a user turn.
- `session.steer` — mid-stream correction.
- `session.interrupt` — abort / cancel.

### Custom tools

Hermes has native MCP support via `ctx.call_mcp`. Config in `config.yaml`:

```yaml
mcp_servers:
  navya-studio:
    command: uv
    args: [run, --with, fastmcp, --with, httpx, fastmcp, run, <abs-path>/navya_mcp/server.py]
    env:
      NAVYA_CONTROL_URL: http://127.0.0.1:<port>
      NAVYA_CONTROL_TOKEN: <token>

plugins:
  entries:
    my-plugin:
      mcp_allowlist: [navya-studio]
```

Tool names are prefixed: `mcp_navya-studio_generate_image`.

### Model / provider config

```yaml
model:
  default: "anthropic/claude-opus-4"
  provider: "custom"
  base_url: "http://localhost:11434/v1"
```

Use `hermes config set model ...` to switch at runtime, or edit `config.yaml`.

### Approvals

Config section:

```yaml
approvals:
  mode: smart          # smart | manual | off
  timeout: 300
  cron_mode: deny
  single_query_mode: deny
  mcp_reload_confirm: true
  destructive_slash_confirm: true
  deny:
    - "git push --force*"
    - "*curl*|*sh*"
```

Hooks:

- `pre_approval_request` — veto/modify approval before it is shown.
- `post_approval_response` — log after the user answers.

The adapter emits `ApprovalRequest` and answers via `approval.respond`.

### Licensing

Hermes Agent is **MIT-licensed** (Nous Research). This is the only third-party
harness that may be bundleable in v1, but model-weight and third-party plugin
licenses still need review.

## OpenClaw

### Architecture

OpenClaw is **gateway-centric**, not stdio-pipeable. The gateway runs a
WebSocket server at `ws://127.0.0.1:18789` by default.

Options:

1. **Native WebSocket adapter** in Navya Studio connects to the gateway and
   speaks JSON-RPC over WS.
2. **MCP bridge:** `openclaw mcp serve --url ... --token-file ...` exposes the
   gateway to an MCP client; studio could point another harness at it, or treat
   OpenClaw as an MCP server itself.

Studio v1 should implement the native WebSocket adapter for full control.

### Gateway connection

Example (Node.js) from docs:

```typescript
import { WebSocket } from "ws";

const ws = new WebSocket("ws://127.0.0.1:18789");

ws.on("open", () => {
  ws.send(JSON.stringify({
    type: "req",
    id: "c1",
    method: "connect",
    params: {
      minProtocol: 4,
      maxProtocol: 4,
      client: { id: "navya-studio", displayName: "Navya Studio", version: "0.1.0", platform: "node", mode: "cli" }
    }
  }));
});
```

### Key RPC methods

| Method | Purpose |
|---|---|
| `connect` | Authenticate / register client |
| `health` | Gateway health |
| `chat.send` | Send a user message (requires `operator.write` scope) |
| `tools.invoke` | Invoke a tool |
| `tools.effective` | List effective tools |
| `gateway probe` | Diagnostic probe |

### `tools.invoke` approvals

`tools.invoke` accepts a `confirm` parameter:

- `confirm: "request"` — prompt the user; on block returns `requiresApproval: true`.
- `confirm: "report"` — just notify; no block.

### chat.send

```typescript
{
  sessionKey: "agent:main:main",
  idempotencyKey: "uuid",
  message: "user prompt",
  queueMode: "steer" | "followup" | "collect" | "interrupt",
  toolBindings: [ ... ]
}
```

### Custom tools

OpenClaw discovers MCP servers. The studio can either:

- Expose `navya-mcp` as an MCP server and let OpenClaw discover it.
- Register tools directly via `tools.effective` / dynamic tool bindings.

### Approvals

Gateway scopes: `operator.read`, `operator.write`, `operator.admin`. The studio
client should connect with `operator.write` to send chat and `operator.read` to
observe.

On `tools.invoke` block, the response contains `requiresApproval: true`. The
adapter must then show a dialog and call the approval response RPC.

### `openclaw mcp serve`

```bash
openclaw mcp serve --url ws://127.0.0.1:18789 --token-file /path/to/token
```

This is an alternative if we want OpenClaw to join as a pure MCP server.

### Licensing

OpenClaw is **MIT-licensed** (OpenClaw Foundation). However, it is
gateway-centric and may be bundleable, but the user must still run the gateway
or the studio must spawn it.

## Cross-harness capability matrix

| Harness | Transport | Persistent | Steer | Abort | Custom tools | Approvals → UI |
|---|---|---|---|---|---|---|
| a-coder-cli | stdio JSONL | yes | yes (`steer`) | yes | TS extension | `extension_ui_request` |
| Claude Code | stdio stream-json | yes | yes (interrupt + input) | yes | MCP + hooks | `PreToolUse` / `PermissionRequest` |
| Codex | app-server JSON-RPC | yes (threads) | yes (`turn/interrupt` + follow-up) | yes | MCP + dynamicTools | `approval_policy` |
| Antigravity | stdio stream-json | per-prompt | **no** | kill proc | MCP (`~/.claude/mcp-servers.json`) | permission_grants |
| Hermes | stdio JSON-RPC | yes | yes (`session.steer`) | yes (`session.interrupt`) | MCP + Python plugins | approval hooks |
| OpenClaw | WebSocket gateway | yes | yes (`queueMode`) | yes (`interrupt`) | MCP + `tools.invoke` | `confirm: request/report` |

## Licensing recommendation for v1

| Harness | Bundle? | Notes |
|---|---|---|
| a-coder-cli | No | User's own install; spawn from PATH |
| Claude Code | No | Anthropic proprietary; spawn user's install |
| Codex | No | OpenAI proprietary; spawn user's install |
| Antigravity | No | Google; spawn user's install |
| Hermes | Maybe | MIT; needs legal + packaging review |
| OpenClaw | Maybe | MIT; gateway-centric; packaging review |

Studio v1 should **require user-installed harnesses** for all six, with
auto-detection and clear "not installed" UI states.

## Open questions / unresolved

1. Exact a-coder-cli version installed and whether `--no-session` is the correct
flag for the installed `pi.exe`.
2. Whether Claude Code supports per-project `.claude/settings.json` or only
`~/.claude/settings.json`.
3. Full Codex app-server event schema (item/updated payloads, approval request
shape) — need live binary to confirm.
4. Antigravity `stream-json` event shapes are undocumented; need the binary to
probe them.
5. Hermes `tui_gateway` exact stdout framing and method return shapes.
6. OpenClaw gateway token lifecycle — how the user creates/regenerates the
token and whether the studio can auto-pair.
