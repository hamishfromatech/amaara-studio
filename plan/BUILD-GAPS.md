# Navya Studio — Build Gaps & Open Questions

What the local build agent **still needs** to complete the project in full,
organized by how to resolve each gap. Read this **before** starting any phase;
update it as you resolve items (strike through or move to a "Resolved" log at
the bottom). Companion to `plan.md` — every item here maps to a phase.

Convention: `[DECIDE]` = the human must choose (changes early phases).
`[PIN]` = confirm an exact value from an existing source. `[DONE]` = look
it up on context7 at the start of the named phase (library ids given).
`[SPEC]` = write a small spec before implementing. `[DATA]` = the human must
provide credentials/asset.

---

## A. Blocking decisions (answer before coding) — `[DECIDE]`

These change Phase 0–6. Resolve first.

1. **Primary target platform.** Dev machine is Windows. Options: (a)
   Windows-first, defer macOS/Linux to Phase 14; (b) cross-platform from day 1
   (more upfront work on sidecar binary suffixes, keyring backends, shell
   quoting). Recommend (a).
2. **Reference harness confirmed = a-coder-cli?** Plan assumes yes. Confirm,
   and confirm a-coder-cli is installable on the build agent's machine (it's
   the user's own `pi-mono`; see licensing below).
3. **Cloud-first vs Local-first default.** Determines the default `Source`
   toggle, what onboarding shows first, and whether sd-server/llama.cpp are
   required for a happy path. Recommend Cloud-first (Navya), Local as opt-in.
4. **Project model: 1 composition per studio project, or many?** Plan's SQLite
   schema assumes 1:1 (a studio project = one HyperFrames composition tree).
   Confirm; if "many", the left rail needs a composition switcher (design.md §12
   lists it as v1.1 — but the schema decision must be made now).
5. **Where do studio projects live on disk?** Pick: a workspace dir under
   OS app-data (`…/Navya Studio/projects/<id>/`) with "Open folder" to
   relocate, vs always user-chosen. Recommend app-data default + relocatable.
6. **Bundle the harnesses or require user-installed?** a-coder-cli is the
   user's own; Claude Code / Codex / Antigravity / Hermes / OpenClaw are
   third-party with their own licenses. Decide: (a) v1 requires the user to
   have the chosen harness installed, studio just spawns it (simplest, no
   licensing); (b) bundle. Recommend (a) for v1; bundling is a Phase 14
   licensing task (see §N).

---

## B. Navya Cloud facts to pin — `[PIN]` + `[DATA]`

**Status:** Research completed → docs/research/navya-cloud-api.md.

The studio talks to Navya as an OpenAI-compatible provider. Don't hardcode
models — use the live catalog.

7. `[PIN]` **Base URL.** From `provider-api/.env` (the deployed URL) — do
   **not** print the value into docs; the adapter reads it from the studio
   settings (user enters it once). Default for dev: `http://localhost:8000`.
8. `[PIN]` **Auth header format.** Confirm the header the proxy expects
   (OpenAI-style `Authorization: Bearer <key>` vs a custom `X-Navya-Key`).
   Check `backend/auth.py` / `backend/services/auth_context.py` and the
   proxy router's key check. The studio's Navya HTTP client must match.
9. `[PIN]` **Model discovery = `GET /public-models`.** It returns
   `{id, name, context_window, kind:"chat"|"image"|"video", supports_vision,
   strengths, usage_level}`. Populate the Models list from this at runtime
   (don't hardcode `navya/auto` etc.). Also `GET /config`,
   `GET /health` for the settings "test connection" button.
10. `[PIN]` **`navya/auto` router.** Selectable as a model id; the studio
    passes it through like any model. Env `AUTO_ROUTER_*` is server-side;
    nothing for the client to do beyond listing it.
11. `[PIN]` **Image gen contract.** `POST /v1/images/generations` is OpenAI
    shape (`model`, `prompt`, `n`, `size`, `quality`, `response_format`).
    Confirm response is `{data:[{url}|{b64_json}]}`. Does Navya return a URL
    or base64? Affects how the studio saves the asset and shows progress.
12. `[PIN]` **Video gen contract.** `POST /v1/videos/generations` is a
    transparent proxy; some providers return a job-id requiring polling.
    Confirm whether Navya's response is synchronous (`{data:[{url}]}`) or a
    job handle + a status endpoint. If polling, the studio needs a poller.
13. `[PIN]` **Audio + embeddings shapes.** `/v1/audio/*` (TTS? speech?) and
    `/v1/embeddings` — confirm exact paths/shapes from
    `backend/routers/audio.py` and `embeddings.py` (voiceover studio is v1.1,
    but the client should be correct from the start).
14. `[PIN]` **BYOK.** `byok_router` exists. Does the studio expose BYOK
    (user's own provider keys, passed through Navya)? Decide yes/no for v1;
    if yes, mirror `BYOK_*` into the studio settings.
15. `[DATA]` **A Navya API key for dev + a test account.** Needed to run
    Gates 4–9 end to end. Stored in the OS keyring only.

---

## C. Per-harness integration specs — `[DONE]` at the start of each adapter

**Status:** Research completed → docs/research/harness-licensing-integration.md.

The adapters (Phase 4 a-coder-cli; Phase 12 Claude; Phase 13 Codex/Hermes/
Antigravity/OpenClaw) each need exact protocol details the build agent must
research **when starting that phase**, not memorize. Context7 library ids:

- a-coder-cli → local docs at `~/.a-coder/lib/a-coder-cli/docs/` (`sdk.md`,
  `rpc.md`, `extensions.md`, `custom-provider.md`, `models.md`).
- Claude Code → `/websites/code_claude` (and `/anthropics/claude-code`).
- OpenAI Codex → `/openai/codex`.
- Antigravity → `/google-antigravity/antigravity-cli`.
- Hermes → `/nousresearch/hermes-agent` (+ `/cclank/hermes-wiki`).
- OpenClaw → `/openclaw/openclaw`.
- FastMCP → `/prefecthq/fastmcp`.

Per harness, research and pin to a `harness-pack/<harness>/SPEC.md`:

16. **a-coder-cli (Phase 4/5):** exact `--mode rpc` flag set; how to pass
    `--cwd` and a custom agent dir for extension/skill/prompt/models.json
    discovery; the `models.json` schema for an OpenAI-compatible provider
    (field names: `baseUrl`? `api`? key reference mechanism — runtime key vs
    env); whether `set_model` accepts arbitrary `provider/model` strings;
    how an extension reads env set on the spawned process (for
    `NAVYA_CONTROL_URL`/`NAVYA_CONTROL_TOKEN`); the exact
    `extension_ui_request`/`response` shapes for `select`/`confirm`/`input`.
    Confirm against the **installed** version (pin it).
17. **Claude Code (Phase 12):** binary name + install; the `--input-format
    stream-json` message schema (user message vs control/interrupt message
    shapes); how `steer`/abort map to streaming-input control messages;
    `PreToolUse`/`PermissionRequest` hook config file location + format +
    the hook response schema (`allow`/`deny`/`interrupt`, `updatedInput`);
    `settings.json` location + `permissions.allow/deny` grammar; MCP config
    (`mcpServers` in settings vs `--mcp-config` flag); system-prompt
    injection (`--append-system-prompt`); `--resume`/`--session-id`
    semantics; exact `stream-json` event types and their fields.
18. **Codex (Phase 13):** `codex app-server` invocation; the **full JSON-RPC
    method list** (the research showed `thread/start`, `command/exec`,
    `dynamicTools` — pin the rest: `thread/send_message`? `thread/stop`?
    notifications); `dynamicTools` function-schema format; `approvalPolicy`
    values and how an approval surfaces as an event/notification the adapter
    must answer; `turn.*`/`item.*` event fields; `--json` exec mode as a
    fallback when app-server isn't available.
19. **Hermes (Phase 13):** how to start the `tui_gateway` headless
    (`python -m tui_gateway.entry`? env/args); the dispatch `method` names
    (the research showed the framing, not the method list — pin it);
    `mcp.servers` YAML schema + `mcp_allowlist`; plugin hook → approval
    mechanism; how a "prompt" is sent and how responses stream.
20. **Antigravity `agy` (Phase 13):** install; the `stream-json` event
    shapes (`init`, `step_update` with thinking/tool_use, `result`) and
    their **fields** (the parser needs these); session continuity (is there
    any `--session`/`--resume`, or strictly one-process-per-turn?); MCP
    config file path and exact format; how to inject a system prompt; how
    approvals surface (MCP `tools.invoke` `confirm`? or `agy`-native?).
21. **OpenClaw (Phase 13):** how to start the local gateway
    (`ws://127.0.0.1:18789`) and get a token; the **full WS JSON-RPC method
    list** (research showed `tools.invoke` only — pin the rest); the
    `confirm: "request"|"report"` flow + `requiresApproval` response;
    streaming event/message shapes; OR confirm `openclaw mcp serve` is
    sufficient and treat OpenClaw as an MCP server behind `navya-mcp` (no
    native adapter) — decide which shape.

---

## D. sd-server & local LLM HTTP contracts — `[DONE]` / `[PIN]`

**Status:** Research completed → docs/research/stable-diffusion-local.md and docs/research/llama-cpp-local.md. Implementation: src-tauri/src/sidecar/llama.rs restored.

22. `[DONE]` **stable-diffusion.cpp `sd-server` HTTP API.** The research
    showed the **start command**, not the HTTP endpoints. Pin at Phase 8
    (context7 `/leejet/stable-diffusion.cpp`, `examples/server/README.md`):
    the txt2img / img2img endpoint paths + request/response JSON; how to
    load/switch models at runtime; how progress is reported (streaming?
    polling a status endpoint?); error shapes; whether it's OpenAI-shaped
    or custom. The `src-tauri/src/sd/` client depends on this.
23. `[PIN]` **sd-server build flavor selection.** CUDA vs Vulkan vs CPU at
    **build** time (the binary is built with one). The installer must detect
    (NVIDIA driver? Vulkan ICD?) and ship the matching binary. Pin the
    detection logic + the per-flavor build flags.
24. `[DATA]` **Diffusion model files.** Which model(s) the studio ships or
    recommends (SDXL? Flux GGUF? SD3?), where to fetch them, licensing, VRAM
    floor per family. The user points at a models dir; the studio should
    offer a one-click "download recommended model" if licensing allows.
25. `[PIN]` **llama.cpp server contract.** Which binary (`llama-server`),
    its OpenAI-compatible endpoints, and crucially **does it support tool
    calls** (`/v1/chat/completions` with `tools`)? A fully-local agent needs
    a tool-calling model. Phase 9 lets the user point at their own server;
    the studio should warn if the server reports no tool support. Pin the
    contract + recommend a tool-capable model for the docs.

---

## E. Remotion / HyperFrames contract — `[DONE]` + `[SPEC]`

**Status:** Research completed → docs/research/hyperframes-render-contract.md.

26. `[DONE]` **HyperFrames CLI exact surface** (context7 skill files at
    `~/.agents/skills/hyperframes-cli/` + `hyperframes-core`): the precise
    flags for `init`, `check`, `preview --background` (and the URL/port it
    prints for the iframe), `render --quality <q> --output <o>`, `snapshot
    --at <t>`, `doctor --json`, `catalog`, `add`. Pin each flag the studio
    invokes.
27. `[SPEC]` **Render worker: shell out vs. SDK.** Decide in Phase 7 whether
    the Node render sidecar shells out to `npx hyperframes render …`
    (simplest, gets all HyperFrames features + skills) or calls
    `@remotion/renderer` `bundle`+`renderMedia` directly (tighter progress,
    no `npx` overhead). Recommend shell out for v1 (the HyperFrames CLI owns
    the project contract); direct SDK is an optimization later.
28. `[DONE]` **Composition → tracks parser.** The Timeline tab parses
    `data-*` timing from `composition.html`. Pin the exact attributes from
    `hyperframes-core` (`data-start`, `data-duration`, `data-media-start`,
    `class="clip"`, track layout, variables, sub-compositions) and whether
    `hyperframes check --json` emits a structured timeline (prefer that over
    HTML parsing if available).
29. `[PIN]` **Chrome headless-shell + FFmpeg versions.** Pin the versions the
    render worker fetches (via `ensureBrowser` / bundled) for reproducible
    renders; surface a `doctor` check in Settings. Affects Phase 14 bundling.
30. `[PIN]` **Preview hot-reload.** How the `preview --background` server
    signals file changes (WebSocket? SSE? reload the iframe on a studio
    event?). The Timeline canvas must not jump the playhead on reload.

---

## F. Tauri implementation decisions — `[SPEC]`

**Status:** Research completed → docs/research/tauri-packaging-security.md.

31. `[SPEC]` **Tauri + tokio + the control server.** Tauri 2 runs its own
    async runtime; the control server (axum/hyper) must share it or run on a
    managed `tokio` runtime. Decide and write the integration pattern in
    Phase 3 (don't spawn a second runtime that fights Tauri's).
32. `[PIN]` **SQLite: `tauri-plugin-sql` vs `rusqlite`.** Plan lists both.
    Pick one (`tauri-plugin-sql` for webview-accessible queries via `Database.execute`,
    or `rusqlite` for Rust-only with the store plugin just for settings).
    Recommend `rusqlite` (Rust owns all DB access; the webview never touches
    SQL directly) + `tauri-plugin-store` for settings.
33. `[PIN]` **Keyring backends per platform.** `keyring` crate: Windows
    Credential Manager, macOS Keychain, Linux Secret Service. Confirm
    headless/CI fallback (env var or a file with 0600) so tests run without a
    keychain. Pin in Phase 1.
34. `[PIN]` **Sidecar binary naming + resolution.** The `externalBin`
    convention (`<name>-<target-triple>`), how `Command::sidecar("name")`
    resolves in dev (PATH fallback) vs bundled, and the capabilities/ACL
    scope entries. Pin in Phase 2.
35. `[PIN]` **Webview CSP.** Must allow the control-server `127.0.0.1`
    origin (fetch from the TS extension? no — that runs in the harness, not
    the webview) and the HyperFrames preview-server origin (iframe). Pin the
    `app.security.csp` value in Phase 6.

---

## G. Control-server design — `[SPEC]`

36. `[SPEC]` **Routes + auth + WS.** Exact routes (`POST /tool/:name`), the
    bearer-token auth, the `WS /events` message shape, the error envelope,
    and how the render worker receives jobs (WS push from the control
    server). Pin in Phase 3.
37. `[SPEC]` **Startup ordering + ephemeral port.** The control server binds
    `127.0.0.1:0`, writes the chosen port + token to the keyring **before**
    the adapter spawns the harness (the harness spawns the MCP server /
    extension which needs `NAVYA_CONTROL_*`). Define the readiness handshake
    (control server up → keyring written → adapter starts) to avoid a race.
38. `[SPEC]` **Concurrency + cancellation.** Multiple tools can run at once
    (e.g. parallel image gens); each tool call has an id; cancel propagates
    to the backend (and to the render sidecar for `render_to_video`). Pin
    the in-flight map + cancellation channel in Phase 3.

---

## H. Project model & on-disk layout — `[SPEC]` (after A.4/A.5)

39. `[SPEC]` **A "project" on disk.** After deciding A.4/A.5: the exact
    directory layout (HyperFrames composition files, `assets/`, `renders/`,
    `BRIEF.md`, plus a `.navya/` dir holding the studio's per-project state
    — selected harness, model, source, render history pointer). The studio
    creates it via `npx hyperframes init` + writes `.navya/`. Pin in Phase 6.
40. `[SPEC]` **Chat history storage.** Per-harness chat history lives in the
    harness's native session format (a-coder-cli session jsonl, Claude
    `--session-id`, Codex threads, …). The studio indexes them by project in
    SQLite (project → session handle). Pin the mapping; don't reimplement
    chat storage.

---

## I. UI implementation choices — `[SPEC]`

41. `[SPEC]` **Component primitives.** shadcn/ui (Radix + Tailwind) vs plain
    Tailwind. Recommend shadcn for accessible dialogs/menus/tabs (matches
    design.md density + theming). Decide in Phase 6.
42. `[SPEC]` **State + the event reducer.** The single `studio://event`
    channel feeds a reducer. Pick: Zustand (recommended for the streaming,
    fine-grained updates) vs Redux Toolkit vs React context. Pin in Phase 6.
43. `[SPEC]` **Chat streaming render.** Token deltas must render <50ms; use a
    virtualized list + a stable per-turn key. Tool cards are keyed by
    `toolCallId`. Pin the chat view implementation in Phase 6.
44. `[SPEC]` **Timeline track parser + preview iframe.** Sandbox the preview
    iframe (`sandbox` attrs + CSP), handle the preview server's hot-reload
    signal without playhead jumps. Pin in Phase 10.
45. `[SPEC]` **Keyboard layer.** A single hotkey manager (e.g. a small
    `hotkeys-js` wrapper) with the map in design.md §10; ensure `⌘K` palette
    doesn't conflict with the webview. Pin in Phase 16.

---

## J. Cross-platform specifics (Windows primary) — `[PIN]`

**Status:** Research completed → docs/research/tauri-packaging-security.md.

46. `[PIN]` **Windows path/quoting.** HyperFrames and the render worker get
    Windows paths (backslashes) in args; the Rust side must normalize to
    forward slashes or quote correctly for the Node child. Pin a helper in
    Phase 7.
47. `[PIN]` **Sidecar binaries on Windows.** `.exe` suffix in `externalBin`,
    console-window suppression for spawned sidecars (no flashing cmd
    windows). Pin in Phase 2/14.
48. `[PIN]` **macOS/Linux parity** is deferred to Phase 14 but the build must
    not use Windows-only APIs in the core (use `std::path`, conditional
    keyring backends, conditional GPU detection).

---

## K. Security & threat model — `[SPEC]`

49. `[SPEC]` **The harness runs `bash`/`write`/`edit`.** The studio trusts the
    harness (user's choice), but scope the harness's working dir to the
    project (cwd = project dir) and surface a deny list (e.g. block reads of
    `.env`, the OS keyring). The "always allow" rules the studio writes into
    each harness's permission config must never include broad `Bash(*)`.
    Pin the deny list + the allow-rule grammar per harness in Phase 11.
50. `[SPEC]` **Secrets handling.** Navya key, control token, any provider
    keys → OS keyring only; never logged; never written to project files or
    harness-pack JSON. Add a test that greps app-data + project dirs for
    plaintext keys after a run. Pin in Phase 15.
51. `[SPEC]` **Control server.** Bind `127.0.0.1` only; token auth; no CORS
    for non-loopback; reject any connection with a missing/wrong token. Pin
    in Phase 3.

---

## L. Conventions, testing, CI — `[SPEC]`

52. `[SPEC]` **Code style gates.** `rustfmt` + `clippy -D warnings`; ESLint +
    Prettier; a shared TS config for `ui/` and `harness-pack/a-coder-cli/`.
    Pin in Phase 0.
53. `[SPEC]` **Test strategy without every harness installed.** Mock
    harnesses (small scripts that speak each protocol's JSONL) so adapter
    tests run in CI without Claude/Codex/etc. installed. Pin in Phase 4;
    one mock per adapter in Phase 12/13.
54. `[SPEC]` **Headless e2e test.** A test that drives the a-coder-cli
    adapter (real, installed) against a fixture HyperFrames project and
    asserts a render produces an mp4 — gated to run only when a-coder-cli +
    the Navya key are present (env-gated in CI). Pin in Phase 15.
55. `[SPEC]` **NOTES.md discipline.** The build agent appends unknowns here
    (plan already says this); add a convention: each entry has a date, a
    phase, a one-line question, and the chosen workaround.
56. `[SPEC]` **Telemetry/privacy.** Off by default; respect HyperFrames'
    `--no-telemetry`, each harness's opt-out, and the studio's own analytics
    (none unless the user opts in). The feedback packager strips absolute
    paths/home dir. Pin in Phase 15.

---

## M. Credentials / environment for development — `[DATA]`

57. `[DATA]` **Navya Cloud base URL + a dev API key** (B.7, B.15).
58. `[DATA]` **a-coder-cli installed and authed** (for the reference path;
    Phase 4+). Confirm the version and pin it.
59. `[DATA]` **Node 22, FFmpeg, Python 3.11, uv** on the build machine.
60. `[DATA]` **An NVIDIA/Vulkan GPU** (optional) to exercise the local
    sd-server path in Phase 8; otherwise develop against Cloud image gen
    and stub sd-server.
61. `[DATA]` **A llama.cpp server** (optional) for the Phase 9 local-LLM
    path; otherwise develop against Navya chat.

---

## N. Licensing & redistribution — `[DECIDE]`

**Status:** Research completed → docs/research/harness-licensing-integration.md. Verdict: user-installed in v1.

62. `[DECIDE]` **Can the installer bundle each harness?** Confirm licenses:
    a-coder-cli (`pi-mono`, the user's own — likely fine), Claude Code
    (Anthropic ToS — almost certainly **not** redistributable; require
    user-installed), Codex (OpenAI ToS — same), Antigravity (Google — check),
    Hermes (Nous Research — OSS, bundle OK), OpenClaw (check). This decides
    A.6 and the Phase 14 bundling list. Default: require user-installed for
    all except a-coder-cli (and Hermes if OSS).
63. `[DECIDE]` **stable-diffusion.cpp license** for bundling `sd-server`
    (it's MIT-ish; confirm) and the **model weights** licenses (SDXL/SD3/
    Flux have different terms — some non-commercial). Decide which model(s)
    the studio can legally download/ship.

---

## Resolve order (do these first)

1. Answer **§A decisions 1–6** (blocking; ~10 min for the human).
2. Pin **§B Navya facts 7–13** by reading `provider-api` routers + `.env`
   (read-only) and asking the human for the dev key (§M.57).
3. For each phase, do the matching **§C research** and write
   `harness-pack/<harness>/SPEC.md` before implementing the adapter.
4. Write the **§F/§G/§H specs** as small header-docs in the relevant phase's
   first PR (don't front-load all of them).
5. Confirm **§N licensing** before Phase 14 (it can stay open until then).

---

## Resolved log (append as you close items)

<!-- Example: 2026-08-24 Phase 0: A.1 decided = Windows-first. -->
## Resolved (2026-08-24, kick-off decisions)

- A.1 **Cross-platform day 1.** Windows + macOS + Linux from the start. Sidecar
  binary per-target-triple suffixes, keyring backends, shell quoting, and GPU
  detection are in-scope from Phase 0/2 (not deferred to Phase 14). Phase 14
  ships all three; per-platform smoke is part of the main build.
- A.2 **a-coder-cli is the day-one reference harness** (confirmed). `AaaCoderCliHarness`
  first; Claude Code in Phase 12 to prove agnosticism.
- A.3 **Cloud-first default.** `Source` defaults to Navya; onboarding shows
  Navya key entry first. sd-server + llama.cpp are opt-in toggles.
- A.4 **Many compositions per studio project.** SQLite gains a `compositions`
  table (project → many compositions); the left rail gets a composition
  switcher (promoted from design.md §12 v1.1 into v1). The timeline/preview
  (Phase 10) is per-composition.
- A.5 **Projects live under OS app-data by default, relocatable.**
  `<appdata>/Navya Studio/projects/<id>/…` with "Open folder" to relocate.
  (Assumed; revisit if it complicates git/VCS workflows.)
- A.6 **Harnesses are user-installed in v1** (pending §N licensing). The
  studio spawns the user's installed harness; only a-coder-cli (user's own)
  is bundled by default. Bundling Claude/Codex/Antigravity/OpenClaw is a
  Phase 14 licensing decision; Hermes may be bundleable if OSS-licensed.

## Resolved log

- **2026-08-26** — §A decisions 1–6 resolved during kick-off and reaffirmed after research.
- **2026-08-26** — §B Navya Cloud research completed → `docs/research/navya-cloud-api.md`.
- **2026-08-26** — §D.22–D.25 sd-server + llama.cpp research completed → `docs/research/stable-diffusion-local.md`, `docs/research/llama-cpp-local.md`.
- **2026-08-26** — §E.26–§E.30 HyperFrames/Remotion research completed → `docs/research/hyperframes-render-contract.md`.
- **2026-08-26** — §F + §J + §K.49–K.51 Tauri/security research completed → `docs/research/tauri-packaging-security.md`.
- **2026-08-26** — §C + §N harness integration + licensing research completed → `docs/research/harness-licensing-integration.md`.
- **2026-08-26** — FastMCP bundling + conformance research completed → `docs/research/fastmcp-bundling.md`.
- **2026-08-26** — `navya-mcp/server.py` updated to FastMCP v4; `pyproject.toml` + `requirements.txt` pinned.
- **2026-08-26** — `src-tauri/src/sidecar/llama.rs` restored and implemented.
