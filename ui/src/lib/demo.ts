/**
 * Dev-only demo mode — a browser design preview harness.
 *
 * When the UI is served by Vite outside Tauri (`pnpm dev` opened in a plain
 * browser), every `invoke` fails and the app would fall back to the
 * onboarding screen, making visual design iteration impossible. When this
 * module's guard matches (DEV build + no Tauri internals), the store seeds a
 * representative snapshot instead of the error state.
 *
 * This NEVER runs in the packaged app: production builds are not DEV and
 * always carry Tauri internals, so the real Rust snapshot always wins there.
 */

import type {StateSnapshot} from './invoke'
import type {ChatMessage} from './store'

export const isDemoMode =
  import.meta.env?.DEV === true &&
  typeof window !== 'undefined' &&
  !('__TAURI_INTERNALS__' in window)

const now = Date.now()

/** A representative in-flight conversation: done turn, tool cards, a live turn. */
export const demoChat: ChatMessage[] = [
  {
    id: 'u1',
    role: 'you',
    content: 'Make a 20-second launch video for our product. Dark, cinematic, confident.',
    tools: [],
    thinking: '',
    mode: 'normal',
  },
  {
    id: 'a1',
    role: 'agent',
    status: 'done',
    content:
      '## Storyboard — `launch-v1`\n\nSix shots, one hook per **two seconds**. The plan:\n\n1. Black frame — a single phosphor cursor blinks twice\n2. Whip through the product surface at cut-the-curve velocity\n3. Hero stat lands with the *waterfall entry*\n4. Tagline pairs with kinetic type\n5. Render finishes on grain\n6. Wordmark holds, slow fade\n\n> Timing rule: the exit velocity of every shot must match the entry velocity of the next — the film is one continuous move.\n\n```html\n<div class="clip" data-start="0" data-duration="2000">\n  <h1 class="hero-title">Direct an agent.</h1>\n</div>\n```\n\n| Shot | Duration | Technique |\n| ---- | -------- | --------- |\n| 1 | 2.0s | cursor ignition |\n| 2 | 1.8s | zoom-through |\n| 3 | 2.4s | waterfall entry |\n\n- [x] Storyboard written to `compositions/launch-v1.html`\n- [x] Draft render queued (`r7f3a91c2`)\n- [ ] High-quality render after review — see the [shot list](https://example.com/shots)\n\nComposition **launch-v1** is written to the project and the draft render is queued.',
    startedAtMs: now - 240_000,
    thinking:
      'The user wants a launch video. Dark and cinematic reads as the darkroom identity — lean on the phosphor cursor motif and keep every seam velocity-matched. Six shots at two seconds each gives a 12-second base; with the tagline beat it lands near 20s.',
    tools: [
      {
        id: 't1',
        name: 'write_file',
        args: {path: 'compositions/launch-v1.html', bytes: 14_203},
        partial: '',
        result: 'wrote compositions/launch-v1.html (14.2 kB)',
        isError: false,
        done: true,
        startedAtMs: now - 210_000,
      },
      {
        id: 't2',
        name: 'render_video',
        args: {composition: 'launch-v1', quality: 'draft', fps: 30},
        partial: 'frame 214/600 · stage: encode',
        result: null,
        isError: false,
        done: false,
        startedAtMs: now - 45_000,
      },
    ],
  },
  {
    id: 'a2',
    role: 'agent',
    status: 'thinking',
    content: '',
    startedAtMs: now - 4_000,
    tools: [],
    thinking: '',
  },
]

/** Full StateSnapshot shape (subset fields the UI reads). */
export function buildDemoSnapshot(): StateSnapshot & {loading: false; error: null} {
  return {
    session: {
      current_project_id: 'p1789441335368070000',
      current_composition_id: 'main',
      harness: 'a-coder-cli',
      model: 'a-coder-cli/glm-5.3-flash',
      source: 'cloud',
    },
    config: {
      amaara_base_url: 'https://api.amaara.industries',
      default_model: 'amaara/auto',
      use_auto_router: true,
      byok: false,
      enabled_harnesses: ['a-coder-cli', 'claude'],
      local_llama_url: 'http://127.0.0.1:8817',
      engine_url: 'https://engine.amaara.industries',
      sd_server_url: null,
      sd_binary_path: '/opt/homebrew/bin/sd-server',
      sd_models_dir: '~/models',
      sd_gpu_backend: 'cpu',
      density: 'comfortable',
      theme: 'dark',
      share_analytics: false,
      mcp_servers: [],
    },
    has_api_key: true,
    projects: [
      {
        id: 'p1789441335368070000',
        name: 'A-Tech launch film',
        dir: '~/Movies/atech-launch',
        created_at_ms: now - 86_400_000 * 3,
        updated_at_ms: now - 3_600_000,
      },
      {
        id: 'p1789441335368070001',
        name: 'HyperFrames sizzle',
        dir: '~/Movies/hf-sizzle',
        created_at_ms: now - 86_400_000 * 9,
        updated_at_ms: now - 86_400_000 * 2,
      },
      {
        id: 'p1789441335368070002',
        name: 'Changelog 0.80',
        dir: '~/Movies/changelog-080',
        created_at_ms: now - 86_400_000 * 16,
        updated_at_ms: now - 86_400_000 * 5,
      },
    ],
    models: [
      {id: 'a-coder-cli/kimi-k2-thinking', name: 'kimi-k2-thinking', kind: 'chat', source: 'harness', active: false},
      {id: 'a-coder-cli/glm-5.3-flash', name: 'glm-5.3-flash', kind: 'chat', source: 'harness', active: true},
      {id: 'cloud/seedream-4', name: 'Seedream 4', kind: 'image', source: 'cloud', active: false},
      {id: 'cloud/flux-2-pro', name: 'Flux 2 Pro', kind: 'image', source: 'cloud', active: false},
      {id: 'local/qwen3-32b', name: 'qwen3-32b', kind: 'chat', source: 'local', active: false},
    ],
    harnesses: [
      {id: 'a-coder-cli', label: 'a-coder-cli', available: true},
      {id: 'claude', label: 'claude-code', available: false},
    ],
    sidecars: [
      {name: 'render-worker', status: 'running', detail: 'pid 41220 · :8711'},
      {name: 'sd-server', status: 'exited', detail: 'not configured'},
    ],
  } as unknown as StateSnapshot & {loading: false; error: null}
}

/** Demo render queue (Renders tab + Home strip). */
export const demoRenders = [
  {
    job_id: 'r7f3a91c2',
    project_id: 'p1789441335368070000',
    composition_id: 'main',
    target: 'mp4',
    quality: 'draft',
    status: 'running' as const,
    started_at_ms: now - 42_000,
    finished_at_ms: null,
    output_path: null,
    error: null,
    progress: {stage: 'encode', frame: 214, total_frames: 600},
  },
  {
    job_id: 'r2b8e44d0',
    project_id: 'p1789441335368070000',
    composition_id: 'main',
    target: 'mp4',
    quality: 'high',
    status: 'done' as const,
    started_at_ms: now - 86_400_000,
    finished_at_ms: now - 86_400_000 + 300_000,
    output_path: '~/Movies/atech-launch/renders/launch-v1.mp4',
    error: null,
  },
]

/** Demo assets (Inspector rail). */
export const demoAssets = [
  {
    id: 'as1',
    project_id: 'p1789441335368070000',
    composition_id: null,
    path: 'assets/hero-frame.png',
    kind: 'image',
    source: 'local',
    prompt: null,
    created_at_ms: now - 3_600_000,
  },
  {
    id: 'as2',
    project_id: 'p1789441335368070000',
    composition_id: null,
    path: 'assets/voiceover.wav',
    kind: 'audio',
    source: 'local',
    prompt: null,
    created_at_ms: now - 7_200_000,
  },
]