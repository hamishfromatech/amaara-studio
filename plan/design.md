# Navya Studio — Design

How the app looks and feels. ASCII layouts for every surface, the interaction
model, states, theming, and keyboard flow. See `ARCHITECTURE.md` for what's
behind each panel.

Navya Studio is a **desktop content-creation studio** (Tauri). The agent
(harness) does the work; the UI is a cockpit for directing it, watching it
think, reviewing what it makes, and rendering it to video.

```
Design tenets
─────────────
• The chat IS the app. Every other panel is a view onto what the agent is
  doing right now. Nothing is a modal wizard you get trapped in.
• Watch the agent work. Tool calls, image drafts, renders all stream live
  into the panels — no "working…" spinners hiding progress.
• One project at a time. A project = a HyperFrames composition tree + its
  assets + its chat history. Switching projects switches everything.
• Harness + model + source are first-class, always-visible choices. The
  studio is harness-agnostic and cloud/local-agnostic by design.
• Native, calm, fast. Tauri chrome, not a web tab. Dark by default.
```

---

## 1. Window shell

Fixed three-pane layout: **Left rail** (project + assets), **Center** (chat or
canvas, tabbed), **Right rail** (inspector + render queue). A persistent top
bar and a status strip.

```
╭──────────────────────────────────────────────────────────────────────────────────╮
│ ⬢ Navya Studio          ◴ black-holes-explainer ▾      ⤓ Render  ◐ Stop         │
│ Harness: a-coder-cli ▾  Model: navya/auto ▾  Source: ● Cloud   ☰ ⚙                 │
├──────────────┬─────────────────────────────────────────────┬─────────────────────┤
│              │                                             │                     │
│   LEFT RAIL  │              CENTER (tabs)                  │     RIGHT RAIL      │
│              │  ┌─ Chat ──┐┌─ Timeline ┐┌─ Assets ┐┌─ Renders ┐                   │
│  Projects    │  │                                             │  Inspector          │
│  Assets      │  │                                             │  Render queue        │
│  Models      │  │                                             │                     │
│              │  │                                             │                     │
├──────────────┴─────────────────────────────────────────────┴─────────────────────┤
│ ● sd-server idle   ● render worker ready   ↑ 3 tools run   ████░░ 40% render       │
╰──────────────────────────────────────────────────────────────────────────────────╯
```

### Top bar (always visible)

```
⬢ Navya Studio          <project-name> ▾          ⤓ Render   ◐ Stop
Harness: a-coder-cli ▾   Model: navya/auto ▾   Source: ● Cloud / ● Local   ☰  ⚙
```

- **Project switcher** (`<name> ▾`) — opens the project list (left rail
  expands). `⤓ Render` kicks the current composition to the render queue with
  last-used quality; long-press / chevron opens quality + target (local /
  docker / cloud / lambda / cloudrun). `◐ Stop` aborts the running agent turn
  (sends normalized `abort`).
- **Harness** picker — the six from the architecture; each shows a small
  capability dot (green = full bidirectional, amber = degraded, e.g.
  Antigravity "no mid-stream steer"). Switching harness mid-project warns that
  chat history is harness-specific.
- **Model** picker — lists models from the active harness's resolved set
  (Navya Cloud models incl. `navya/auto`, plus local llama.cpp models). The
  studio's adapter wrote the provider entries; this just calls `set_model`.
- **Source** — `● Cloud` (Navya) / `● Local` (sd-server + llama.cpp). Drives
  where `generate_image` routes. Affects the status strip's sidecar dots.

### Status strip (always visible)

```
● sd-server idle   ● render worker ready   ↑ 3 tools run   ████░░ 40% render
```

Live health of the three sidecars (harness / render / sd-server) + MCP tool
server, the count of in-flight tool calls, and the active render's progress
bar. Clicking a sidecar dot opens its log drawer.

---

## 2. Left rail — Project, Compositions, Assets, Models

Four stacked, collapsible sections. Default widths ~240px; user-draggable.

> **Project model (kick-off decision):** a studio project holds **many
> compositions** (e.g. one brief → a 30s cut and a 60s cut). The left rail has
> a composition switcher directly under the project; the timeline/preview,
> assets filter, and renders tab all key off the **selected composition**.

```
╭─ COMPOSITIONS ────────────╮
│ ▾ black-holes-explainer   │
│   ● 30s cut     ✓         │
│   ○ 60s cut               │
│   + New composition       │
╰───────────────────────────╯
```
```
╭─ PROJECT ────────────────────╮
│ ▾ black-holes-explainer      │
│   ├ composition.html         │
│   ├ BRIEF.md                 │
│   ├ STORYBOARD.md            │
│   ├ assets/                  │
│   │  ├ img/                  │
│   │  │  ├ scene1.png         │
│   │  │  └ scene2.png         │
│   │  └ audio/                │
│   │     └ vo.mp3             │
│   └ renders/                 │
│      └ out-v1.mp4            │
│                              │
│ + New project                │
│ ⤴ Open folder                │
╰──────────────────────────────╯
╭─ ASSETS ─────────────────────╮
│ ┌────┐ ┌────┐ ┌────┐         │
│ │img │ │img │ │img │         │
│ └────┘ └────┘ └────┘         │
│ ┌────┐ ┌────┐ ┌────┐         │
│ │img │ │img │ │ +  │         │
│ └────┘ └────┘ └────┘         │
╰──────────────────────────────╯
  ↑ thumbnail grid (generated images, imported footage)
  ↑ "+" = generate / import
╭─ MODELS ─────────────────────╮
│ ▾ Cloud (Navya)              │
│   ● navya/auto   ✓           │
│   ○ Qwen3-32B-TEE            │
│   ○ dall-e-3   (image)       │
│   ○ sora-2     (video)       │
│ ▾ Local                      │
│   ○ llama3-8b  (llama.cpp)   │
│   ○ sd-xl      (sd-server)   │
╰──────────────────────────────╯
  ↑ active model marker (✓)
```

- **Project tree** is the real on-disk HyperFrames project (the agent's
  `cwd`). Clicking a file opens it in the center canvas tab; double-clicking
  `assets/img/scene1.png` previews + shows provenance (which prompt/turn made
  it). "Open folder" reveals it in the OS file manager.
- **Assets** grid is the studio's media library — generated images, imported
  footage, voiceover. Generated images carry a small badge (`cloud`/`local`)
  and the prompt that made them on hover.
- **Models** mirrors the active harness's model list, grouped Cloud vs Local
  by the `Source` toggle. Image/video models are tagged `(image)`/`(video)`
  and are only pickable when relevant.

---

## 3. Center — Chat tab (default)

The cockpit. A streaming conversation with the agent, where every tool call
renders inline as a live card rather than disappearing into a log.

```
╭─ Chat ─╮ ┌ Timeline ┐ ┌ Assets ┐ ┌ Renders ┐
┇                                                                                ┇
┇  you · 14:02                                                                   ┇
┇  make a 30s faceless explainer about black holes, calm tone, navy palette       ┇
┇                                                                                ┇
┇  ◷ agent · a-coder-cli · navya/auto · thinking…                                ┇
┇  Routing to /faceless-explainer. Running the intent interview…                 ┇
┇  ┌─ 🛠 tool: write ──────────────────────────────────────────────────────┐    ┇
┇  │ BRIEF.md  ·  612 bytes                                                  │    ┇
┇  │ ─ subject: black holes  ·  tone: calm  ·  palette: navy  ·  dur: 30s     │    ┇
┇  │ [view file]                                                             │    ┇
┇  └────────────────────────────────────────────────────────────────────────┘    ┇
┇  ┌─ 🛠 tool: generate_image ────────────────────────────────────────────┐    ┇
┇  │ prompt: "event horizon, deep navy, minimal, no text"   ● Cloud        │    ┇
┇  │ ▓▓▓▓▓▓▓▓▓▓░░░░  generating  ───────────────────────  12s              │    ┇
┇  │ ┌────────┐  result                                       │    ┇
┇  │ │  img  │  saved → assets/img/scene1.png                │    ┇
┇  │ └────────┘  [regenerate] [edit prompt] [use local]       │    ┇
┇  └────────────────────────────────────────────────────────────────────────┘    ┇
┇  ┌─ 🛠 tool: bash ───────────────────────────────────────────────────────┐    ┇
┇  │ $ npx hyperframes lint                                                │    ┇
┇  │ ✓ 0 errors · 2 warnings (typography contrast on scene 3)              │    ┇
┇  └────────────────────────────────────────────────────────────────────────┘    ┇
┇  ◷ agent · draft storyboard written. Want me to preview before render?          ┇
┇                                                                                ┇
┇  ┌────────────────────────────────────────────────────────────────────────┐    ┇
┇  │ Reply to agent…                                              [▶ Send] │    ┇
┇  │ ⚙ steer · follow-up · attach image · @asset                          │    ┇
┇  └────────────────────────────────────────────────────────────────────────┘    ┇
┇                                                                                ┇
╰────────────────────────────────────────────────────────────────────────────────╯
```

### Anatomy of the chat

- **Turn headers** carry harness + model + a live state dot: `◷ thinking`,
  `▶ tool-calling`, `✓ done`, `✗ error`. The dot color matches the status
  strip sidecar it relates to.
- **Tool cards** are the heart of the UI. Each is a compact, live block:
  - `write`/`edit` → file name, size, a one-line diff summary, "view file".
  - `generate_image` → the prompt, Cloud/Local badge, a progress bar while
    generating, the thumbnail when done, save path, and quick actions:
    `regenerate`, `edit prompt`, `use local` (re-run via sd-server).
  - `bash` → the command + a folded stdout with exit status. Long output
    collapses to "12 lines · expand".
  - `render_to_video` → becomes a render-queue card (see §5).
- **Compose box** has three send modes (mirroring the harness contract):
  `▶ Send` (normal), `↪ steer` (interrupt mid-turn), `⏎ follow-up` (queue
  after turn). On a degraded harness with no mid-stream steer, `steer` is
  shown as `↪ abort + re-send` and labelled "coarse".
- `@asset` mentions drop image/media references into the prompt; `@file`
  references a project file so the agent gets it as context.

---

## 4. Center — Timeline tab

The assembled HyperFrames composition as a preview + a track view, synced to
the chat. When the agent edits `composition.html`, this updates live.

```
╭─ Chat ──╮ ┌─ Timeline ──────────────────────────────────────────────────────────┐
┇  00:00          05:00          10:00         15:00         20:00         25:00   ┇
┇ ┌────────────────────────────────────────────────────────────────────────────┐ ┇
┇ │                                                                            │ ┇
┇ │                       [ ▶  live preview canvas  1280×720 ]                  │ ┇
┇ │                                                                            │ ┇
┇ └────────────────────────────────────────────────────────────────────────────┘ ┇
┇ ── tracks ─────────────────────────────────────────────────────────────────── ┇
┇ ▌title    ▓▓▓▓▓▓▓▓▓▓ "What is a black hole?"           ░░░░░░░░░░░░░░░░░░░░░ ┇
┇ ▌scene1   ▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ ┇
┇ ▌scene2                ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ ┇
┇ ▌scene3                              ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░ ┇
┇ ▌vo       ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ ┇
┇ ▌bgm      ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ ┇
┇ ───────────────────────────────────────────────────────────────────────────── ┇
┇  ▌title   "What is a black hole?"            ▌vo  "...a region where…"        ┇
┇   t=07.2s  ◀ ▶  ■  [edit in chat]  [snapshot]  [add keyframe]                 ┇
└────────────────────────────────────────────────────────────────────────────────┘
```

- **Preview canvas** is the HyperFrames `preview --background` render, shown
  paused at the playhead. Play scrubs; the agent's edits hot-reload.
- **Tracks** come straight from the composition's `data-*` timing (the
  `hyperframes-core` contract): title cards, scenes, voiceover, music. Each
  clip is clickable → opens in the right-rail inspector (clip start/duration,
  media source, animation rules, variables).
- **"edit in chat"** is the key cross-panel link: selecting a clip and hitting
  it pre-fills the compose box with a targeted instruction ("shorten scene 2
  by 2s") — the agent edits the composition; you don't hand-edit HTML here.
- **snapshot** runs `hyperframes snapshot --at <t>` and pins the frame to the
  assets grid for review/feedback.

---

## 5. Center — Renders tab + Right rail render queue

Renders are long, so they live in a queue with rich progress, not a blocking
dialog. The right rail shows the queue; the center Renders tab shows the
selected job's detail.

```
╭─ Chat ─╮ ┌ Timeline ┐ ┌ Assets ┐ ┌─ Renders ──────────────────────────────────────┐
┇ ─ render queue ──────────────────────────────────────────────────────────────── ┇
┇ ▶ out-v1.mp4   high · local   ████░░░░░░ 38%   frame 432/1140   ~1m20s left   ⏸ ✕ ┇
┇ ○ out-draft.mp4 draft · local   queued                                         ┇
┇ ✓ out-v0.mp4    high · local   done  ·  2m11s  ·  14.2 MB   [open] [reveal]    ┇
┇ ✗ out-480p.mp4  high · cloud   failed — Chrome timeout  [retry] [logs]        ┇
┇ ───────────────────────────────────────────────────────────────────────────── ┇
┇ ─ selected: out-v1.mp4 ─────────────────────────────────────────────────────── ┇
┇ target    local  ▾        quality  high  ▾       codec  h264  ▾                 ┇
┇ size 1280×720  ▾       fps 30  ▾           frames 1140                        ┇
┇ progress  ████████████░░░░░░░░░░  38%   432/1140                              ┇
┇ stage  rendering frames (Chrome headless)                                     ┇
┇ ┌────────────────────────────────────────────────────────────────────────────┐┇
┇ │  [ last rendered frame preview ]                                           │┇
┇ └────────────────────────────────────────────────────────────────────────────┘┇
┇ logs (folded) ▾                                                                ┇
┇  [⤓ Reveal file when done]   [⏸ Pause]   [✕ Cancel]                          ┇
└────────────────────────────────────────────────────────────────────────────────┘
```

- The **queue** is the studio's render sidecar jobs (local), plus cloud
  targets (lambda / cloudrun / HeyGen `cloud render`) the user can opt into.
- Each job shows target, quality, a live bar with frame count + ETA, and
  `⏸ ✕` controls. Completed jobs expose `[open]` (play in-app) and
  `[reveal]` (OS folder). Failed jobs show a trimmed cause + `[retry]`/`[logs]`.
- The **detail** pane shows the configurable render knobs (size, fps, codec,
  quality, target), the last rendered frame as a live preview, and the
  folded log stream from the Node sidecar.

---

## 6. Right rail — Inspector

Contextual: shows details for whatever is selected (a clip, an asset, a model,
a render). Default when nothing is selected: project summary.

```
╭─ INSPECTOR ─────────────╮
│ ▸ Project               │
│   black-holes-explainer │
│   30s · 1280×720 · 30fps│
│   harness a-coder-cli   │
│   model navya/auto      │
│   4 scenes · 6 assets   │
│                         │
│ ▸ Selected: scene2 clip │
│   start  06.0s  ▾       │
│   dur    06.0s  ▾       │
│   media  scene2.png     │
│   motion  reveal-up     │
│   ─ variables ──        │
│   title  "The horizon"  │
│   palette #0a1a2f       │
│   [edit in chat]        │
│                         │
│ ▸ Generation source     │
│   ● Cloud (Navya)       │
│   ○ Local (sd-server)   │
│   ┌─ last image ──────┐ │
│   │      [thumb]      │ │
│   └───────────────────┘ │
│   prompt: "event…"      │
│   12.3s · $0.002        │
╰─────────────────────────╯
```

The inspector is the bridge between the visual canvas and the chat: every
value here is editable, and "edit in chat" turns the change into a natural
instruction for the agent rather than a silent hand-edit.

---

## 7. Settings

Reached from the `⚙` in the top bar. Left-nav sections; right shows the form.

```
╭─ SETTINGS ───────────────────────────────────────────────────────────────────────╮
│ ▸ Navya Cloud            │  Navya Cloud                                          │
│ ▸ Local models           │  Endpoint   https://api.navya.cloud            [test] │
│ ▸ Harnesses              │  API key    ●●●●●●●●●●●●●●●●              [reveal] [rot]│
│ ▸ Appearance             │  Default model  navya/auto  ▾                          │
│ ▸ Shortcuts              │  ☑ Use navya/auto router                               │
│ ▸ Advanced               │  ☐ BYOK to underlying providers                         │
│                          │  Usage this month  4.2M tokens · $3.40  [view usage]    │
│                          │                                                       │
│                          │  Local models                                          │
│                          │  LLM (llama.cpp server)  http://localhost:8080  [start] │
│                          │  Image (sd-server)      http://localhost:7860  [start] │
│                          │  sd-server binary  …/binaries/sd-server.exe  [browse]  │
│                          │  GPU backend  ● CUDA  ○ Vulkan  ○ CPU                  │
│                          │  Models dir  …/models  [browse]                        │
│                          │                                                       │
│                          │  Harnesses (order = menu + default)                    │
│                          │  ☑ a-coder-cli   (reference, full bidirectional)        │
│                          │  ☑ Claude Code   (stream-json + hooks)                 │
│                          │  ☑ Codex         (app-server threads)                  │
│                          │  ☐ Antigravity   (print mode, no steer)                │
│                          │  ☑ Hermes        (JSON-RPC gateway)                    │
│                          │  ☐ OpenClaw      (WebSocket gateway)                   │
╰──────────────────────────────────────────────────────────────────────────────────╯
```

- **Navya Cloud** — endpoint, key (stored in the OS keyring, never echoed),
  default model, router toggle, BYOK toggle (maps to Navya's `byok_router`),
  live usage pulled from the dashboard stats.
- **Local models** — llama.cpp server URL + sd-server URL + binary/model
  paths + GPU backend. `[start]` launches the sidecar from the status strip.
- **Harnesses** — which are enabled and the order they appear in the top-bar
  picker. Each row notes its capability tier from the architecture matrix.

---

## 8. Empty / onboarding states

### First launch (no project, no Navya key)

```
╭──────────────────────────────────────────────────────────────────────────────────╮
│                                                                                  │
│                              ⬢  Navya Studio                                     │
│                                                                                  │
│                  A content-creation studio powered by AI.                        │
│             Direct an agent. Watch it make. Render to video.                     │
│                                                                                  │
│        ┌─────────────────────────┐    ┌─────────────────────────┐                 │
│        │  Connect Navya Cloud    │    │   Start with a local     │                │
│        │  paste your API key  →  │    │   model  (sd.cpp + LLM)  │                │
│        └─────────────────────────┘    └─────────────────────────┘                │
│                                                                                  │
│                        …or  open an existing project                             │
│                                                                                  │
│   Pick a harness first:  ● a-coder-cli  ○ Claude Code  ○ Codex  ○ Hermes  ○ …     │
╰──────────────────────────────────────────────────────────────────────────────────╯
```

### New project (the agent runs the intent interview in-chat)

```
╭─ Chat ─────────────────────────────────────────────────────────────────────────╮
┇  agent · What do you want to make?                                             ┇
┇  A few questions and I'll write the brief, then the composition.               ┇
┇                                                                                ┇
┇  • Subject?  e.g. "black holes", "our Q3 launch", a URL…                        ┇
┇  • Length & format?  30s faceless explainer / 60s promo / title card …          ┇
┇  • Tone & palette?  calm + navy / bold + neon / minimal …                       ┇
┇  • Narration?  TTS voiceover / captions only / music bed …                       ┇
┇                                                                                ┇
┇  ┌────────────────────────────────────────────────────────────────────────┐     ┇
┇  │ Answer…                                                     [▶ Send]  │     ┇
┇  └────────────────────────────────────────────────────────────────────────┘     ┇
╰────────────────────────────────────────────────────────────────────────────────╯
```

The interview is just the agent's first messages — no separate wizard. The
`/faceless-explainer` (etc.) workflow writes `BRIEF.md`, which appears in the
left rail, and the composition starts populating the Timeline tab as the agent
works. The user can interject with `steer` at any point.

---

## 9. States & feel

### Agent turn states (the chat turn-header dot)
```
  ◷ thinking        grey pulse    model is reasoning
  ▶ tool-calling    blue          a tool is running (card animates in)
  ⏸ waiting-user   amber         an approval/extension_ui_request is open
  ✓ done           green         turn complete
  ✗ error          red           needs retry or intervention
  ↻ retry          blue spin     auto-retry on transient error
```

### Approvals (harness asks, studio answers natively)
When a harness raises an approval (a-coder-cli `extension_ui_request`,
Claude `PermissionRequest`, Codex `approvalPolicy`, OpenClaw
`confirm: request`), the studio renders a **native Tauri dialog** rather than
text in chat:

```
        ╭─ Permission ───────────────────────────────╮
        │ a-coder-cli wants to run:                  │
        │ $ npx hyperframes render --quality high    │
        │                                            │
        │ ☐ Always allow this command                │
        │        [ Allow ]   [ Deny ]   [ Edit ]     │
        ╰────────────────────────────────────────────╯
```

"Always allow" writes a scoped rule into the harness's native permission
config (a-coder-cli extension, Claude `settings.json` `permissions.allow`,
etc.) so the studio gets quieter over time.

### Transitions & motion
- Panels do **not** slide/wizard. Only the center tab changes; rails stay put.
- Tool cards **fade/slide in** from the chat as they start; progress bars
  animate width only (no spinners hiding state).
- A completed image card does a single 120ms scale-in on the thumbnail.
- Render progress is a plain bar; on completion the row does a green flash.
- Switching harness/model is instant and non-destructive (chat history is
  retained per harness; a toast notes degradations, e.g. "Antigravity has no
  mid-stream steer — steering will abort + re-send").

### Performance feel
- Chat streaming is token-by-token, never batched > 50ms.
- The Timeline preview hot-reloads on file change via the HyperFrames preview
  server; the playhead never jumps.
- Sidecars start lazy: sd-server only spins up on the first *local* image;
  the render sidecar stays warm but idle between jobs.
- All sidecar logs are one click away (status-strip dot → log drawer) and
  never on the critical path of the UI.

---

## 10. Keyboard

```
  ⌘K / Ctrl+K     command palette (jump to project, file, action, model)
  ⌘N             new project
  ⌘Enter         send chat (normal)
  ⌘⇧Enter        send as steer (interrupt mid-turn)
  ⌘⌥Enter        send as follow-up (queue after turn)
  ⌘.             stop / abort current agent turn
  ⌘R             render current composition (last quality)
  ⌘⇧R            render… (choose quality + target)
  ⌘1..4          center tabs: chat / timeline / assets / renders
  ⌘\             toggle left rail     ⌘/   toggle right rail
  ⌘,             settings
  Space          play/pause timeline preview at playhead
  J K L          timeline shuttle (←  play  →)
  ⌘E             "edit in chat" the selected clip/asset
  ?               show shortcuts overlay
```

---

## 11. Theming

- **Dark by default** (the studio is for long rendering sessions). A light
  theme ships; both are Tauri-native (real window chrome), not web CSS only.
- Palette anchors: a deep "studio" charcoal background, a single accent
  (Navya violet/blue) reserved for the active/running state and primary
  actions, neutral greys for structure, semantic colours only for state dots
  (blue=running, amber=waiting, green=done, red=error).
- Typography: a neutral UI sans for chrome; a mono for code/bash/output and
  the timeline timecode. No decorative type in chrome.
- Density: a "comfortable" default and a "compact" option for laptops; the
  rails and tool cards respect it.

---

## 12. Surfaces not yet drawn (v1.1 candidates)

- **Batch render** view (`--batch rows.json`) — a table of variable-driven
  renders with per-row progress, fed from a CSV/JSON the user edits.
- **Asset provenance graph** — which prompt → which image → which clip used
  it, for auditing generations.
- **Cost / usage dashboard** — Navya token + image + video spend per project.
- **Voiceover studio** — record or TTS a track and place it on the VO track
  without leaving the app (uses Navya `/v1/audio` or local TTS).
- **Multi-composition project** — if §6 question 5 resolves to "many
  compositions per project", the left rail gains a composition switcher.