# FastMCP Conformance Testing & Python/uv Bundling — Navya Studio

**Date:** 2026-08-24  
**Sources:** `plan/BUILD-GAPS.md` §C FastMCP, §L.53 conformance testing, Phase 14 packaging; context7 `/prefecthq/fastmcp` (v3.2.0 / v3.2.4, v4.0.0b3 prerelease docs).  
**Goal:** Pin exact answers for how `navya-mcp` is built, tested, wired into harnesses, and packaged so Phase 12/13/14 can implement without guessing.

---

## 1. Executive summary

`navya-mcp` is the Python FastMCP server that exposes Navya Studio tools to the five MCP-capable harnesses (Claude Code, Codex, Antigravity, Hermes, OpenClaw). It is intentionally a **thin proxy**: each `@mcp.tool` forwards to the Rust loopback control server via `httpx`. Tool logic stays in Rust.

Key pins from this research:

- Target FastMCP **v4 stable** (currently `4.0.0b3` prerelease). Pin `fastmcp==<version>` and `httpx>=0.28` in `navya-mcp/requirements.txt`; do not leave `>=0.1.0`.
- `mcp.run()` defaults to **stdio** transport; no args needed for the harness use-case.
- `Context` is injected by **type annotation**, conventionally as the **last** parameter, or explicitly via `ctx: Context = CurrentContext()`.
- `ctx.report_progress(progress, total, message)` — **not** a single string.
- Conformance test uses `fastmcp.utilities.tests.run_server_in_process` (subprocess stdio isolation) plus an in-memory `Client(mcp)` for unit tests.
- Per-harness wiring is the same `mcpServers` JSON shape: `command: uv`, args `run --with fastmcp --with httpx fastmcp run <abs path>/navya_mcp/server.py`, env `NAVYA_CONTROL_URL` + `NAVYA_CONTROL_TOKEN`.
- Phase 14 default packaging: **ship via `uv`**, letting the harness materialise the env on first run. Offline fallback: bundle a standalone Python + pinned wheels in `resources/`.
- Windows: spawn through `uv` (or `uv.exe`), avoid `python.exe` console flashes, quote absolute paths with spaces, use `python` / `python3` per platform.

The current `navya_mcp/server.py` and `requirements.txt` are **out of sync** with FastMCP 4 (see §9). This report does not implement fixes but flags the exact changes needed.

---

## 2. FastMCP server API (the `navya-mcp` implementation contract)

### 2.1 Tool registration — `@mcp.tool`

```python
from fastmcp import FastMCP, Context

mcp = FastMCP("navya-studio-tools")

@mcp.tool
def greet(name: str) -> str:
    """Description becomes the tool description."""
    return f"Hello, {name}!"
```

Decorator signature (FastMCP 4 docs):

```
@mcp.tool(
    name: str | None = None,
    description: str | None = None,
    title: str | None = None,
    tags: set[str] | None = None,
    icons: list[Icon] | None = None,
    annotations: ToolAnnotations | dict | None = None,
    meta: dict[str, Any] | None = None,
    timeout: float | None = None,
    version: str | int | None = None,
    output_schema: dict[str, Any] | None = None,
    run_in_thread: bool = True,
)
```

For Navya Studio we only need the defaults plus the docstring for descriptions. Tool names default to the Python function name. No versioning is required for v1.

Class-method registration is also possible via `mcp.add_tool()` if needed later, but function decorators are the v1 pattern.

### 2.2 Context injection

`Context` is injected automatically when a parameter is type-hinted as `Context`. Best practice in FastMCP 4 is to place it **last** in the signature:

```python
from fastmcp import FastMCP, Context

mcp = FastMCP("navya-studio-tools")

@mcp.tool
async def generate_image(project_id: str, prompt: str, ctx: Context) -> dict:
    await ctx.info(f"Generating image for project {project_id}")
    await ctx.report_progress(progress=0, total=100, message="Submitting to control server")
    # ... httpx call ...
    await ctx.report_progress(progress=100, total=100, message="Done")
    return {"path": "..."}
```

Alternative explicit injection (works the same):

```python
from fastmcp.dependencies import CurrentContext

@mcp.tool
async def generate_image(project_id: str, prompt: str, ctx: Context = CurrentContext()) -> dict:
    ...
```

The `Context` parameter is **removed from the MCP client schema**, so harnesses only see `project_id`, `prompt`, etc.

### 2.3 Context methods we will use

| Method | Signature | Purpose |
|--------|-----------|---------|
| `ctx.info(msg)` | `await ctx.info(message: str)` | Log info to client / UI tool card |
| `ctx.debug(msg)` | `await ctx.debug(message: str)` | Debug log |
| `ctx.warning(msg)` | `await ctx.warning(message: str)` | Warning log |
| `ctx.error(msg)` | `await ctx.error(message: str)` | Error log |
| `ctx.report_progress(...)` | `await ctx.report_progress(progress, total=None, message=None)` | Per-tool progress |
| `ctx.read_resource(uri)` | `await ctx.read_resource(uri: str) -> ResourceResult` | Read an MCP resource (payload in `.contents[0].content`) |
| `ctx.request_id` | property | Current request id |
| `ctx.client_id` | property | Current client id |
| `ctx.set_state / get_state` | `await ctx.set_state(key, value, serializable=True)` | Session/request state |

Note: `ctx.info/debug/warning/error` in FastMCP 4 require a **string**, not arbitrary JSON values.

### 2.4 Progress reporting

Exact signature:

```python
await ctx.report_progress(progress: float, total: float | None = None, message: str | None = None)
```

Example:

```python
await ctx.report_progress(progress=10, total=100, message="Generating image...")
```

### 2.5 Returning image content

Use `fastmcp.utilities.types.Image` for in-chat thumbnails:

```python
from fastmcp.utilities.types import Image

@mcp.tool
async def generate_image(..., ctx: Context) -> Image:
    png_bytes = await generate_thumbnail(...)
    return Image(data=png_bytes, format="png")
```

Supported construction:

- `Image(path="/path/to/file.png")` — MIME type inferred from extension.
- `Image(data=bytes, format="png")` — `format` is required when passing raw bytes.

Accepted image formats (MCP standard): `png`, `jpeg`, `gif`, `webp`.

For file assets (renders, snapshots) we should normally return a `dict` with `path`/`url`/`metadata` and let the studio UI render the thumbnail, using `Image` only when the harness needs to display binary media inline.

### 2.6 Running the server — `mcp.run()`

Default = stdio transport:

```python
if __name__ == "__main__":
    mcp.run()                       # stdio
```

HTTP/SSE for testing only:

```python
mcp.run(transport="http", host="127.0.0.1", port=8000)
mcp.run(transport="streamable-http", host="127.0.0.1", port=8000)
mcp.run(transport="sse", host="127.0.0.1", port=8000)   # legacy compatibility
```

For the studio harness path the server **must** run over **stdio** because the harness spawns it as a child process. HTTP is useful only for local debugging or the conformance test harness.

Default settings from `fastmcp.settings.FastMCPSettings`:

- `transport: "stdio"`
- `host: "127.0.0.1"`
- `port: 8000`
- `streamable_http_path: "/mcp"`
- env override prefix: `FASTMCP_` (e.g. `FASTMCP_TRANSPORT`, `FASTMCP_PORT`)

`pyproject.toml` `[tool.fastmcp]` section can override these defaults, but for harness use we leave them as stdio.

---

## 3. Conformance testing (BUILD-GAPS §L.53)

### 3.1 Two test layers

1. **In-process unit/integration tests** — fastest, no network/subprocess overhead.
   ```python
   from fastmcp import FastMCP, Client

   mcp = FastMCP("TestServer")

   @mcp.tool
   def greet(name: str) -> str:
       return f"Hello, {name}!"

   async def test_greet():
       async with Client(mcp) as client:
           result = await client.call_tool("greet", {"name": "World"})
           assert result.data == "Hello, World!"
   ```

2. **Subprocess stdio isolation** — required to verify the server actually speaks spec-correct MCP stdio and that harness config fragments work.
   ```python
   from fastmcp.utilities.tests import run_server_in_process
   from fastmcp import FastMCP, Client
   from fastmcp.client.transports import StreamableHttpTransport

   def run_server(host: str, port: int) -> None:
       server = FastMCP("TestServer")

       @server.tool
       def greet(name: str) -> str:
           return f"Hello, {name}!"

       server.run(host=host, port=port)

   @pytest.fixture
   async def http_server():
       with run_server_in_process(run_server, transport="http") as url:
           yield f"{url}/mcp"

   async def test_http_transport(http_server: str):
       async with Client(transport=StreamableHttpTransport(http_server)) as client:
           tools = await client.list_tools()
           assert "greet" in [t.name for t in tools]
   ```

`run_server_in_process` is a context manager that:

- starts the server in a fresh process,
- allocates a free port,
- returns the URL,
- terminates the process on exit.

Signature:

```python
run_server_in_process(server_fn: Callable[..., None], *args: Any, **kwargs: Any) -> Generator[str, None, None]
```

### 3.2 Recommended Navya Studio conformance test

Add `tests/test_navya_mcp_conformance.py` in `navya-mcp/`:

- Use `run_server_in_process` with a stdio-spawning function or use an in-memory `Client(mcp)` plus a harness-config parser.
- For every studio tool (`generate_image`, `render_to_video`, `list_local_models`, `set_generation_source`, `get_project_state`, `snapshot`, `open_in_folder`):
  1. `list_tools()` asserts the tool is present with the expected JSON schema.
  2. `call_tool(name, args)` asserts the control server receives the correct payload (mock control server, or spin the real Rust control server on `127.0.0.1:0`).
  3. Assert progress/logging messages are emitted if the tool uses `ctx.report_progress` / `ctx.info`.
- Also parse each harness config fragment under `harness-pack/{claude-code,codex,hermes,antigravity,openclaw}/` and assert the generated `mcpServers` entry would launch `navya_mcp/server.py` with `NAVYA_CONTROL_URL` and `NAVYA_CONTROL_TOKEN`.

This satisfies BUILD-GAPS §L.53: *"Mock harnesses … so adapter tests run in CI without Claude/Codex/etc. installed."* The mock is the FastMCP `Client` + the harness config JSON, not a full harness binary.

---

## 4. Installing / wiring into harnesses

### 4.1 FastMCP CLI install helpers

FastMCP can write config files for common clients:

```bash
# Claude Desktop
fastmcp install claude-desktop navya_mcp/server.py --name "Navya Studio" \
  --env NAVYA_CONTROL_URL=http://127.0.0.1:PORT \
  --env NAVYA_CONTROL_TOKEN=TOKEN \
  --with fastmcp --with httpx

# Claude Code
fastmcp install claude-code navya_mcp/server.py --name "Navya Studio" \
  --env NAVYA_CONTROL_URL=http://127.0.0.1:PORT \
  --env NAVYA_CONTROL_TOKEN=TOKEN \
  --with fastmcp --with httpx

# Cursor
fastmcp install cursor navya_mcp/server.py --name "Navya Studio" \
  --env NAVYA_CONTROL_URL=http://127.0.0.1:PORT \
  --env NAVYA_CONTROL_TOKEN=TOKEN \
  --with fastmcp --with httpx
```

Common `fastmcp install` options (from docs):

- `--name <server-name>` — custom name in `mcpServers`.
- `--env KEY=VALUE` — repeated env vars (string values).
- `--env-file <path>` — load env from `.env`.
- `--with <pkg>` — repeated extra packages (e.g. `fastmcp`, `httpx`).
- `--with-editable <path>` — local editable package.
- `--with-requirements <path>` — requirements file.
- `--python <version>` — e.g. `3.11`.
- `--config-path <path>` — Claude Desktop config dir override.
- `--workspace <path>` — Cursor project workspace.

For Navya Studio the adapter should **not** rely on `fastmcp install` at runtime; instead it should generate the equivalent `mcpServers` JSON fragment and (for Claude Code) call `claude mcp add`, or write the appropriate config file. `fastmcp install` is useful for manual setup docs and for CI/tests.

### 4.2 Manual `mcpServers` JSON shape

This is the canonical fragment that every harness config uses, regardless of client:

```json
{
  "mcpServers": {
    "navya-studio": {
      "command": "uv",
      "args": [
        "run",
        "--with", "fastmcp",
        "--with", "httpx",
        "fastmcp",
        "run",
        "/absolute/path/to/navya_mcp/server.py"
      ],
      "env": {
        "NAVYA_CONTROL_URL": "http://127.0.0.1:<PORT>",
        "NAVYA_CONTROL_TOKEN": "<TOKEN_FROM_KEYRING>"
      }
    }
  }
}
```

Key fields:

- `command` — `"uv"` (or `"uv.exe"` on Windows when not on PATH as `uv`).
- `args` — must contain `fastmcp run <server.py>`. The path should be absolute to avoid cwd ambiguity.
- `env` — **string key/value pairs only**. `NAVYA_CONTROL_URL` and `NAVYA_CONTROL_TOKEN` are injected by the adapter before spawning the harness.

Optional fields supported by the MCP standard and some clients:

- `timeout` — startup timeout in ms.
- `description` — human-readable description.

### 4.3 Client-specific config locations

| Client | Config file | Notes |
|--------|-------------|-------|
| Claude Desktop | macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`<br>Windows: `%APPDATA%\Claude\claude_desktop_config.json` | Needs full restart after edit. |
| Claude Code | `~/.claude/mcp-servers.json` or `claude mcp add` | `claude mcp add navya-studio -e NAVYA_CONTROL_URL=... -e NAVYA_CONTROL_TOKEN=... -- uv run --with fastmcp --with httpx fastmcp run /abs/path/navya_mcp/server.py` |
| Cursor | Global: `~/.cursor/mcp.json`<br>Workspace: `<project>/.cursor/mcp.json` | `--workspace <path>` writes project-specific. |
| Codex | Uses Claude-Code-compatible `~/.claude/mcp-servers.json` typically, or its own `mcpServers` block. | Confirm in Phase 13. |
| Antigravity | `~/.claude/mcp-servers.json` (Claude-Code-compatible file). | Per RESEARCH.md. |
| Hermes | `mcp.servers` YAML config, not JSON. | Phase 13 research needed; it is a different shape. |
| OpenClaw | Either native WS gateway or `openclaw mcp serve --url ... --token-file ...` treated as another MCP server. | Phase 13 decision pending. |

### 4.4 What the harness sees

When the harness spawns the server it runs effectively:

```bash
NAVYA_CONTROL_URL=http://127.0.0.1:<PORT> \
NAVYA_CONTROL_TOKEN=<TOKEN> \
uv run --with fastmcp --with httpx fastmcp run /abs/path/navya_mcp/server.py
```

The server then:

1. Reads `NAVYA_CONTROL_URL` and `NAVYA_CONTROL_TOKEN` from `os.environ`.
2. Registers tools via FastMCP.
3. Speaks stdio MCP to the harness.
4. Forwards each tool call to `POST {NAVYA_CONTROL_URL}/tool/{tool_name}` with `Authorization: Bearer {NAVYA_CONTROL_TOKEN}`.

---

## 5. Running with `uv` (development & harness path)

### 5.1 The exact command

```bash
uv run --with fastmcp --with httpx fastmcp run navya_mcp/server.py
```

Equivalent for a specific Python version:

```bash
uv run --python 3.11 --with fastmcp --with httpx fastmcp run navya_mcp/server.py
```

With explicit transport for local debugging:

```bash
uv run --with fastmcp --with httpx fastmcp run navya_mcp/server.py --transport http --port 8000
```

`fastmcp run` accepts:

```bash
fastmcp run <server.py>[:<mcp_var>] [--transport stdio|http|sse|streamable-http] [--host HOST] [--port PORT] [--log-level LEVEL]
```

Default transport is stdio, so no flags are needed for the harness path.

### 5.2 How `uv run --with` resolves dependencies

- `uv run --with fastmcp --with httpx ...` creates an **ephemeral isolated environment** containing those packages plus their transitive deps, without modifying the current project venv.
- It downloads from PyPI on first invocation and caches wheels in uv’s global cache (`~/.cache/uv` on Linux/macOS, `%LOCALAPPDATA%\uv` on Windows).
- Subsequent invocations are fast because uv reuses the resolved/installed packages.
- If a `pyproject.toml` is present in the cwd, uv may also read project metadata. To avoid surprises, pass `--no-project` or run from a neutral directory, or pin exact versions in `navya-mcp/pyproject.toml`.

### 5.3 Pinned versions for reproducibility

`navya-mcp/requirements.txt` should be:

```text
fastmcp==4.0.0b3
httpx>=0.28.0
```

Replace `4.0.0b3` with the latest stable v4 once released. Pinning prevents the current `>=0.1.0` from resolving to an ancient, API-incompatible version.

If using the prerelease before stable is available, `pyproject.toml` should add a constraint for `fastmcp-slim`:

```toml
[project]
dependencies = ["fastmcp==4.0.0b3"]

[tool.uv]
constraint-dependencies = ["fastmcp-slim==4.0.0b3"]
```

---

## 6. Bundling & offline packaging (Phase 14)

### 6.1 Recommended default: ship via `uv`

The installer does **not** need to bundle Python. It bundles only:

- `navya_mcp/server.py`
- `navya-mcp/pyproject.toml`
- `navya-mcp/requirements.txt`

The harness (or the adapter on first spawn) runs:

```bash
uv run --with fastmcp --with httpx fastmcp run <resources>/navya_mcp/server.py
```

`uv` downloads Python + wheels on first run and shows progress in the studio UI. This matches `plan.md` Phase 14 recommendation.

Pros:

- Smaller installer.
- No platform-specific Python build.
- Easy updates (bump pin in `requirements.txt`).

Cons:

- First launch requires internet.
- `uv` must be present on the host (BUILD-GAPS §M.59 lists it as a dev dependency; Phase 14 can bundle `uv` itself in `resources/` or require it).

### 6.2 Offline fallback: standalone Python + pinned wheels

For fully offline installs, bundle:

- A **standalone Python distribution** (e.g. `python-build-standalone` / `indygency/python-standalone` / `embeddable` CPython) per target platform in `resources/python/<triple>/`.
- Pre-downloaded `.whl` files for `fastmcp`, `httpx`, and their transitive dependencies in `resources/python/wheels/`.
- A small bootstrap script that installs the wheels into a local directory and runs `fastmcp run`.

Example layout:

```text
resources/
  python/
    x86_64-pc-windows-msvc/
      python.exe
    x86_64-unknown-linux-gnu/
      bin/python3
    ...
    wheels/
      fastmcp-4.0.0b3-py3-none-any.whl
      httpx-0.28.1-py3-none-any.whl
      ...
  navya_mcp/
    server.py
```

The harness `mcpServers` fragment then points to the bundled interpreter:

```json
{
  "mcpServers": {
    "navya-studio": {
      "command": "C:\\Program Files\\Navya Studio\\resources\\python\\x86_64-pc-windows-msvc\\python.exe",
      "args": [
        "-m", "fastmcp",
        "run",
        "C:\\Program Files\\Navya Studio\\resources\\navya_mcp\\server.py"
      ],
      "env": {
        "NAVYA_CONTROL_URL": "http://127.0.0.1:<PORT>",
        "NAVYA_CONTROL_TOKEN": "<TOKEN>"
      }
    }
  }
}
```

Alternatively, keep `uv` but ship with `--offline` and a pre-populated cache:

```bash
uv run --offline --with fastmcp --with httpx fastmcp run ...
```

This requires the uv cache to be bundled and pointed at via `UV_CACHE_DIR`.

### 6.3 Decision for Phase 14

- **Primary:** `uv` on first run (smaller, simpler).
- **Offline variant:** bundle standalone Python + wheels only if the user opts into an offline installer or enterprise deployment requires it.
- `navya-mcp` is **not** a `tauri.conf.json` `externalBin` because it is Python, not a compiled per-target binary.

---

## 7. Harness config fragments

### 7.1 Common template

All MCP-capable harnesses should receive the same base fragment, adapted only for file path / quoting:

```json
{
  "mcpServers": {
    "navya-studio": {
      "command": "uv",
      "args": [
        "run",
        "--with", "fastmcp==4.0.0b3",
        "--with", "httpx>=0.28.0",
        "fastmcp",
        "run",
        "{{ABS_PATH_TO_NAVYA_MCP_SERVER_PY}}"
      ],
      "env": {
        "NAVYA_CONTROL_URL": "{{NAVYA_CONTROL_URL}}",
        "NAVYA_CONTROL_TOKEN": "{{NAVYA_CONTROL_TOKEN}}"
      }
    }
  }
}
```

The adapter replaces:

- `{{ABS_PATH_TO_NAVYA_MCP_SERVER_PY}}` — absolute path to the bundled or source `navya_mcp/server.py`.
- `{{NAVYA_CONTROL_URL}}` — `http://127.0.0.1:<PORT>` from the running control server.
- `{{NAVYA_CONTROL_TOKEN}}` — bearer token from the OS keyring.

### 7.2 Claude Code fragment

In addition to writing `~/.claude/mcp-servers.json`, the adapter can use the CLI:

```bash
claude mcp add navya-studio \
  -e NAVYA_CONTROL_URL=http://127.0.0.1:<PORT> \
  -e NAVYA_CONTROL_TOKEN=<TOKEN> \
  --scope user \
  -- uv run --with fastmcp --with httpx fastmcp run /abs/path/navya_mcp/server.py
```

### 7.3 Cursor fragment

Write `~/.cursor/mcp.json` (global) or `<project>/.cursor/mcp.json` (workspace):

```json
{
  "mcpServers": {
    "navya-studio": {
      "command": "uv",
      "args": ["run", "--with", "fastmcp", "--with", "httpx", "fastmcp", "run", "/abs/path/navya_mcp/server.py"],
      "env": {
        "NAVYA_CONTROL_URL": "http://127.0.0.1:<PORT>",
        "NAVYA_CONTROL_TOKEN": "<TOKEN>"
      }
    }
  }
}
```

### 7.4 Environment variable contract

The Rust control server writes to the keyring **before** spawning the harness. The adapter reads the same keyring entries and injects them into the harness environment so they flow to the MCP server child process.

Required env:

- `NAVYA_CONTROL_URL` — full base URL of the control server (e.g. `http://127.0.0.1:49234`).
- `NAVYA_CONTROL_TOKEN` — bearer token the server expects.

No other env vars are required by `navya-mcp` v1.

---

## 8. Platform-specific concerns

### 8.1 Windows

- **Binary name:** `uv.exe`, not `uv`. If `uv` is on PATH the harness resolves it; otherwise use the absolute path to the bundled `uv.exe`.
- **Python executable:** in a standalone distribution use `python.exe`. Do **not** use `python3.exe` or `pythonw.exe` unless you intentionally want no console window.
- **Console window flash:** when the harness spawns `uv run ...`, request `CREATE_NO_WINDOW` (or equivalent) on Windows so users do not see a flashing cmd window.
- **Path quoting:** absolute paths to `navya_mcp/server.py` may contain spaces (`C:\Program Files\Navya Studio\...`). Pass them as a single array element in `args`; do not concatenate into a shell string. FastMCP harnesses generally pass args verbatim, so JSON array form is safest.
- **Path separators:** the `mcpServers` JSON should contain Windows backslashes in absolute paths, but uv and Python accept forward slashes too. Use `std::path::Path` formatting in Rust and do not manually replace separators.

### 8.2 macOS / Linux

- **Binary names:** `uv` and `python3`. Some systems may also have `python`.
- **Config locations:** `~/.claude/mcp-servers.json`, `~/.cursor/mcp.json`, `~/Library/Application Support/Claude/claude_desktop_config.json`.
- **Permissions:** bundled standalone Python may need executable bit set in the installer.

### 8.3 uv cache path

- Windows: `%LOCALAPPDATA%\uv`
- macOS/Linux: `~/.cache/uv`

If the studio needs to clear/warm the cache, use these locations.

---

## 9. Gaps in the current `navya-mcp` code

The existing files (`navya-mcp/navya_mcp/server.py`, `navya-mcp/requirements.txt`, `navya-mcp/pyproject.toml`) do not yet conform to the pinned API. These must be fixed in Phase 12/13:

| Issue | Current state | Required fix |
|-------|---------------|--------------|
| FastMCP version | `requirements.txt`: `fastmcp>=0.1.0` | Pin to `fastmcp==4.0.0b3` (or stable v4). |
| `httpx` version | `httpx>=0.24.0` | Bump to `httpx>=0.28.0` to match FastMCP 4 internals. |
| `Context` position | `async def generate_image(ctx: Context, project_id: str, ...)` | Move `ctx` to last position, e.g. `async def generate_image(project_id: str, prompt: str, ctx: Context)`. |
| `report_progress` call | `ctx.report_progress("generating image...")` | Change to `await ctx.report_progress(progress=..., total=..., message="generating image...")`. |
| Sync `httpx` in async tool | `with httpx.Client() as client:` inside async def | Use `httpx.AsyncClient` and `await client.post(...)`, or keep sync but mark tool non-async. |
| `pyproject.toml` | Minimal, no deps | Add `[project] dependencies`, `[tool.uv] constraint-dependencies` if on v4 prerelease. |
| Tool registration style | `@mcp.tool()` | Use `@mcp.tool` (no parens) unless overriding metadata. |

None of these are blockers for this research report, but they should be the first changes when implementing Phase 12.

---

## 10. Action items

1. **Pin versions:** update `navya-mcp/requirements.txt` and `pyproject.toml` to FastMCP v4 + modern httpx.
2. **Fix server.py:** move `ctx` to last param, fix `report_progress`, switch to async httpx, add `Image` returns where appropriate.
3. **Write conformance test:** `tests/test_navya_mcp_conformance.py` using `run_server_in_process` (stdio) and `Client(mcp)` (in-process), asserting `list_tools` + `call_tool` for every studio tool and validating each harness config fragment.
4. **Create harness-pack config fragments:** per-harness files under `harness-pack/{claude-code,codex,antigravity,hermes,openclaw}/` containing the templated `mcpServers` JSON shown in §7.
5. **Phase 14 packaging decision:** default to `uv` first-run; design offline fallback as standalone Python + wheels only if requested.
6. **Windows spawn detail:** ensure the adapter passes `CREATE_NO_WINDOW` when launching harnesses that spawn `uv` children.
