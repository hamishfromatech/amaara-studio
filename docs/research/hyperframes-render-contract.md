# Navya Studio — HyperFrames / Remotion Render Contract

Research report for the render sidecar and timeline integration. Based on
HyperFrames CLI docs, local skill files, and Remotion renderer source.

## Executive summary

- The render sidecar should shell out to `npx hyperframes render` for v1. It is
the simplest path and gives us all HyperFrames features (Docker, cloud,
quality, variables, lint) with deterministic JSON progress.
- Direct `@remotion/renderer` (`bundle` + `renderMedia`) is an optimization for
later; it requires us to replicate HyperFrames' seek/capture logic.
- Preview: `npx hyperframes preview --background --port <port>` starts a local
server. The URL is `http://127.0.0.1:<port>/`. Hot reload is automatic.
- Timeline parsing: read `index.html` + `compositions/*.html` and look for
`data-start`, `data-duration`, `data-track-index`, `data-composition-id`,
`data-composition-src`, etc. No structured JSON from `hyperframes check` was
confirmed for v1.
- Environment: Node ≥ 22, Chrome headless-shell, FFmpeg. Use `hyperframes doctor`
for readiness; `hyperframes browser ensure` for the pinned Chrome binary.

## HyperFrames CLI commands used by the studio

### `npx hyperframes init <project>`

Scaffolds a new HyperFrames project. The agent should run this when creating a
new studio project. Options from the CLI skills (see
`hyperframes-cli/references/init-and-scaffold.md`):

- `--skip-skills` is currently ignored; set `HYPERFRAMES_SKIP_SKILLS=1` env to
opt out in CI.
- `--yes` / `--no-prompt` may exist for headless use.
- A stable `HYPERFRAMES_RUN_ID` should be used for all commands in the same
verification loop.

### `npx hyperframes catalog [options]`

Browse the registry.

| Option | Meaning |
|---|---|
| `--type block\|component` | Filter by type |
| `--tag <tag>` | Filter by tag |
| `--json` | Structured output for agents |
| `--human-friendly` | Interactive picker |

### `npx hyperframes add <name_or_tag> [options]`

Install a block/component into the project.

| Option | Meaning |
|---|---|
| `--dir <dir>` | Target project directory |
| `--json` | Print written files + snippet |
| `--no-clipboard` | Skip clipboard (for CI) |

Examples:

```bash
npx hyperframes add claude-code-window
npx hyperframes add shader-wipe
npx hyperframes add captions --json --no-clipboard
```

### `npx hyperframes lint [dir] [options]`

Checks `index.html` and `compositions/` for structural issues.

| Option | Meaning |
|---|---|
| `--verbose` | Include info-level findings |
| `--json` | Machine-readable output |

```bash
npx hyperframes lint ./my-project
npx hyperframes lint ./my-project --json
```

### `npx hyperframes check [dir] [options]`

Validates the project (similar to lint but typically stricter). Use `--json` for
agent output if available. (Exact flags need to be verified at runtime.)

### `npx hyperframes preview [dir] [options]`

Starts the live preview server.

| Option | Meaning |
|---|---|
| `--background` | Keep running after command exits (headless mode) |
| `--foreground` | Keep attached in non-interactive shells |
| `--port <n>` | Server port (default `3002`) |
| `--open` / `--no-open` | Open browser |
| `--json` | Emit JSON status |

```bash
npx hyperframes preview --background --port 4567
```

The Studio Timeline tab should embed the preview URL in an iframe:

```
http://127.0.0.1:4567/
```

Hot reload is automatic on file changes.

### `npx hyperframes snapshot [project] [options]`

Capture PNG frames at specific times.

| Option | Meaning |
|---|---|
| `--at <t1,t2,...>` | Timestamps in seconds, comma-separated |
| `--frames <n>` | Evenly spaced frames (default 5) |
| `--output, -o <dir>` | Output directory (default `<project>/snapshots`) |
| `--end / --no-end` | Always capture an end-of-timeline frame |
| `--zoom <selector or x,y,w,h>` | Crop region |
| `--zoom-scale <n>` | Pixel density for zoom crops (default 3) |
| `--angle <front|iso|top|side|yaw,pitch>` | Orthogonal 3D camera |
| `--describe "<question>"` / `false` | Gemini vision analysis; `false` to opt out |
| `--timeout <ms>` | Wait for runtime (default 5000) |
| `--browser-gpu` | Enable browser GPU |
| `--proxy <url>` | Browser proxy |

```bash
npx hyperframes snapshot --at 3.0,10.5,18.0 --output ./snapshots
```

### `npx hyperframes render [project] [options]`

The main render command. This is the recommended integration surface for the
render sidecar.

| Option | Meaning |
|---|---|
| `--output, -o <path>` | Output video path |
| `--quality draft\|high` | Render quality |
| `--docker` | Render in Docker for reproducibility |
| `--strict` | Fail on lint errors |
| `--strict-all` | Fail on lint errors + warnings |
| `--variables '<json>'` | Override composition variables |
| `--variables-file <path>` | JSON file with variables |
| `--strict-variables` | Fail on undeclared/mismatched variables |
| `--browser-timeout <seconds>` | Puppeteer navigation timeout (default 60) |
| `--quiet` | Suppress verbose output |
| `--browser-gpu` | Enable GPU in browser |
| `--proxy <url>` | Browser proxy |

Examples:

```bash
# Local draft
npx hyperframes render --quality draft --output ./out.mp4

# Local high
npx hyperframes render --quality high --output ./final.mp4

# Docker for reproducibility
npx hyperframes render --docker --strict --output ./final.mp4

# With variable overrides
npx hyperframes render \
  --variables '{"title":"Q4 Report","theme":"dark"}' \
  --strict-variables \
  --output q4.mp4
```

Render targets (beyond local):

| Target | Command |
|---|---|
| Cloud (HeyGen-hosted) | `npx hyperframes cloud render ...` |
| AWS Lambda | `npx hyperframes lambda render ...` |
| Google Cloud Run | `npx hyperframes cloudrun render --wait ...` |
| Batch | `npx hyperframes render --batch rows.json --output "renders/{name}.mp4"` |

### `npx hyperframes doctor [options]`

Environment diagnostics.

| Option | Meaning |
|---|---|
| `--json` | CI/agent output; exit 0, gate on payload `ok` |

```bash
npx hyperframes doctor --json | jq -e '.ok' > /dev/null
```

Checks include: version compatibility, CPU/memory/disk, env vars,
FFmpeg/FFprobe status, Chrome status, Docker status.

### `npx hyperframes browser <subcommand>`

Manages the pinned Chrome binary.

| Subcommand | Meaning |
|---|---|
| `ensure` | Find or download pinned Chrome |
| `ensure --force` | Re-fetch even if cached |
| `path` | Print binary path |
| `clear` | Remove cached download |

```bash
npx hyperframes browser ensure
npx hyperframes browser path
```

### Telemetry / env vars

- `HYPERFRAMES_NO_TELELETRY=1` disables anonymous telemetry globally.
- `HYPERFRAMES_API_KEY` / `HEYGEN_API_KEY` for cloud features.
- `HEYGEN_API_URL` to point at a different backend.
- `HYPERFRAMES_SKIP_SKILLS=1` to skip skill checks in CI.
- `HYPERFRAMES_RUN_ID` to correlate commands in a verification loop.
- `PRODUCER_BROWSER_GPU_MODE=hardware` (or `--browser-gpu`) for GPU-accelerated
browser rendering.

## Composition / timeline parser

A HyperFrames composition is a set of HTML files. The root `index.html` contains
clips/sub-compositions. Clips are `<div>` elements with `data-*` timing
attributes.

Required attributes for a clip/block:

| Attribute | Meaning |
|---|---|
| `data-composition-id` | Unique id for this clip |
| `data-composition-src` | Path to the block/component HTML |
| `data-start` | Start time in seconds |
| `data-duration` | Duration in seconds |
| `data-track-index` | Layer / z-order |
| `data-width` | Width in pixels |
| `data-height` | Height in pixels |

Example from the registry docs:

```html
<div
  id="chart"
  data-composition-id="data-chart"
  data-composition-src="compositions/data-chart.html"
  data-start="2"
  data-duration="8"
  data-track-index="2"
  data-width="1920"
  data-height="1080"
>
</div>
```

Variables can be declared on the root `<html>` element:

```html
<html
  data-composition-variables='[
    {"id":"title","label":"Title","type":"string","default":"Hello"},
    {"id":"theme","label":"Theme","type":"enum","options":[{"value":"light","label":"Light"},{"value":"dark","label":"Dark"}],"default":"light"}
  ]'
>
```

The `TimelineTab` should:

1. Read `index.html`.
2. Find all elements with `class="clip"` or `data-composition-src`.
3. Extract `data-start`, `data-duration`, `data-track-index`.
4. Render tracks by track index, with clips positioned horizontally by time.
5. Selecting a clip populates the Inspector with id, src, timing, dimensions.

## Remotion renderer details (for future direct SDK use)

If we later bypass `npx hyperframes render` and call Remotion directly:

```ts
import { bundle } from '@remotion/bundler';
import { selectComposition, renderMedia } from '@remotion/renderer';

const serveUrl = await bundle({ entryPoint: './src/index.ts', webpackOverride });
const composition = await selectComposition({ serveUrl, id, inputProps });

await renderMedia({
  composition,
  serveUrl,
  codec: 'h264',
  outputLocation: './out.mp4',
  inputProps,
  onProgress: ({ progress, renderedFrames, encodedFrames, frames }) => {
    // emit progress to UI
  },
});
```

Key `renderMedia` options:

| Option | Type | Notes |
|---|---|---|
| `outputLocation` | string \| null | Output path |
| `codec` | `Codec` | e.g. `'h264'` |
| `composition` | `VideoConfig` | From `selectComposition` |
| `inputProps` | object | Passed to composition |
| `crf` | number \| null | Quality |
| `imageFormat` | `VideoImageFormat` | `'jpeg'` / `'png'` |
| `jpegQuality` | number | Deprecated `quality` renamed |
| `frameRange` | `FrameRange` \| null | Partial render |
| `everyNthFrame` | number | Slow-motion / effect |
| `onProgress` | function | `{ progress, renderedFrames, encodedFrames, frames }` |
| `onStart` | function | `{ frameCount, parallelEncoding, resolvedConcurrency }` |
| `chromiumOptions` | object | Headless flags |
| `scale` | number | Pixel scale |
| `concurrency` | number \| string \| null | Render parallelism |
| `cancelSignal` | `CancelSignal` | Abort render |

Browser management:

```ts
import { ensureBrowser } from '@remotion/renderer';

await ensureBrowser({
  onBrowserDownload: () => ({
    version: '149.0.7790.0',
    onProgress: ({ percent }) => console.log(`${Math.round(percent * 100)}%`),
  }),
});
```

## Recommended studio integration

### Render sidecar (v1)

Use a Node script that:

1. Receives a render job from the Rust control server (via WebSocket or stdin).
2. Runs `npx hyperframes render --quality <q> --output <out> [--docker]` in the
project directory.
3. Parses `stderr`/`stdout` for progress if available, or polls the output file.
4. Emits `RenderEvent` back to Rust: `Started`, `Progress { frame, total, eta }`,
`Completed { path }`, `Failed { error }`, `Cancelled`.
5. On `npx hyperframes` completion, reveals the file path.

### Render targets surfaced in the UI

- **Local** — default; runs on the user's machine.
- **Docker** — `hyperframes render --docker --strict`.
- **Cloud** — `hyperframes cloud render` (HeyGen-hosted).
- **Lambda** — `hyperframes lambda render`.
- **CloudRun** — `hyperframes cloudrun render --wait`.

### Preview/Timeline (v1)

1. When the user switches to the Timeline tab, ensure `npx hyperframes preview --background --port <port>` is running for the current composition.
2. Embed `http://127.0.0.1:<port>/` in an iframe with `sandbox` attributes.
3. Parse `index.html` to build the track view.
4. On file changes the preview server hot-reloads; do not reset the playhead
unnecessarily.

### Snapshot (v1)

Use `npx hyperframes snapshot --at <t> --output <dir>` to capture frames for
the Assets panel. The agent can also call `snapshot` as a tool.

## Open questions / unresolved

1. Does `npx hyperframes check --json` emit a structured timeline? The docs do
not confirm this; timeline parsing should fall back to HTML `data-*` attributes.
2. What is the exact JSON progress shape emitted by `npx hyperframes render`?
Need to run it live to confirm. If none, we may need to wrap with Remotion SDK
or poll file size.
3. What pinned Chrome version does HyperFrames currently require? The Remotion
example showed `149.0.7790.0`, but HyperFrames may pin a different version.
Run `npx hyperframes browser path` to inspect.
4. Is `ffmpeg` fetched automatically by HyperFrames, or must the sidecar call
`npx remotion install ffmpeg`? The `doctor` command checks for it; the sidecar
should run `doctor` on first spawn.
