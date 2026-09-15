/**
 * Typed Tauri invoke wrappers — the real UI ↔ Rust bridge.
 *
 * Each function maps to a #[tauri::command] registered in src-tauri/src/lib.rs.
 */

import {invoke as tauriInvoke} from '@tauri-apps/api/core'

export async function invoke<T>(command: string, payload?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(command, payload) as Promise<T>
}

// --- Types mirrored from Rust -----------------------------------------------

export interface Session {
  current_project_id: string | null
  current_composition_id: string | null
  harness: string
  model: string
  source: string
}

export interface AmaaraConfig {
  amaara_base_url: string
  default_model: string
  use_auto_router: boolean
  byok: boolean
  enabled_harnesses: string[]
  local_llama_url: string
  engine_url: string
  sd_server_url: string | null
  sd_binary_path: string
  sd_models_dir: string
  sd_gpu_backend: 'cuda' | 'vulkan' | 'cpu'
  density: 'comfortable' | 'compact'
  theme: 'dark' | 'light'
  share_analytics: boolean
  /** User-configured additional MCP servers (Tools → MCP servers). */
  mcp_servers: McpServerConfig[]
}

/** One user-configured MCP server entry (mirrors config::McpServerConfig). */
export interface McpServerConfig {
  name: string
  command: string
  args: string[]
  /** [key, value] pairs — serde serializes the tuple vec as an array of pairs. */
  env: [string, string][]
  enabled: boolean
}

/** Built-in amaara-mcp server + runtime status (mirrors commands::McpStatus). */
export interface McpStatus {
  mcp_dir: string
  server_py: string
  server_exists: boolean
  uv_available: boolean
  uv_version: string | null
  control_url: string | null
  has_control_token: boolean
  mcp_servers: McpServerConfig[]
}

/** A saved composer attachment (mirrors commands::AttachmentInfo). */
export interface AttachmentInfo {
  id: string
  name: string
  path: string
  rel_path: string
  kind: 'image' | 'file'
  size_bytes: number
  created_at_ms: number
}

export interface ModelEntry {
  id: string
  name: string
  kind: string // chat | image | video
  source: string // cloud | local
  active: boolean
}

export interface HarnessInfo {
  id: string
  label: string
  enabled: boolean
  steer: boolean
  abort: boolean
  persistent: boolean
  available: boolean
  /** Detected binary path (when found on PATH). */
  path?: string
  /** First line of `<bin> --version`, when readable. */
  version?: string
  /** Install instructions link from the harness descriptor. */
  install_url?: string
  /** Docs link from the harness descriptor. */
  docs_url?: string
}

/** One-line tooltip for a harness row: detection provenance or setup hint. */
export function harnessHint(h: HarnessInfo): string {
  if (h.available) {
    return [h.version, h.path].filter(Boolean).join('\n') || 'installed'
  }
  const base = `${h.label} is not installed or not on PATH.`
  return h.install_url ? `${base}\nInstall: ${h.install_url}` : base
}

export interface SidecarHealth {
  name: string
  status: string
  detail: string | null
}

export interface ProjectRow {
  id: string
  name: string
  dir: string
  created_at_ms: number
  harness: string
  model: string
  source: string
}

export interface StateSnapshot {
  session: Session
  config: AmaaraConfig
  projects: ProjectRow[]
  models: ModelEntry[]
  sidecars: SidecarHealth[]
  has_api_key: boolean
  harnesses: HarnessInfo[]
  render_count: number
}

export interface Clip {
  id: string
  src: string | null // block/component HTML path, when referenced
  start_s: number // data-start (seconds)
  duration_s: number // data-duration (seconds)
  track_index: number // data-track-index (layer / z-order)
  width: number | null
  height: number | null
  /** Raw `data-composition-variables` JSON string, when present. */
  variables: string | null
}

export interface TimelineTrack {
  index: number
  name: string
  label: string
  start_ms: number
  duration_ms: number
  media: string | null
}

export interface TimelineState {
  project_id: string
  composition_id: string
  /** Composition file that was parsed (relative to the project dir). */
  entry: string
  clips: Clip[]
  tracks: TimelineTrack[]
  duration_ms: number
  width: number
  height: number
  fps: number
}

export interface SnapshotResult {
  asset_id: string
  path: string
  timecode_ms: number
}

export interface PreviewStatus {
  running: boolean
  port: number | null
}

export interface RenderJob {
  job_id: string
  project_id: string
  composition_id: string
  target: string
  quality: string
  status: 'queued' | 'running' | 'done' | 'failed' | 'cancelled'
  started_at_ms: number | null
  finished_at_ms: number | null
  output_path: string | null
  error: string | null
  /** Live progress pushed via studio://event (not persisted). */
  progress?: {stage: string; frame: number; total_frames: number | null} | null
}

// --- Commands ---------------------------------------------------------------

export const Commands = {
  getState: () => invoke<StateSnapshot>('get_state'),
  getConfig: () => invoke<AmaaraConfig>('get_config'),
  saveConfig: (config: AmaaraConfig) => invoke<AmaaraConfig>('save_config', {config}),
  setApiKey: (key: string) => invoke<void>('set_api_key', {key}),
  clearApiKey: () => invoke<void>('clear_api_key'),
  hasApiKey: () => invoke<boolean>('has_api_key'),

  newProject: (name: string, dir: string) => invoke<ProjectRow>('new_project', {args: {name, dir}}),
  listProjects: () => invoke<ProjectRow[]>('list_projects'),
  openProject: (projectId: string) => invoke<void>('open_project', {projectId}),
  deleteProject: (projectId: string) => invoke<boolean>('delete_project', {projectId}),

  setModel: (model: string) => invoke<Session>('set_model', {model}),
  setSource: (source: string) => invoke<Session>('set_source', {source}),
  setHarness: (harness: string) => invoke<Session>('set_harness', {harness}),
  listModels: () => invoke<ModelEntry[]>('list_models'),

  sendPrompt: (msg: string, mode: string) => invoke<void>('send_prompt', {msg, mode}),
  steer: (msg: string) => invoke<void>('steer', {msg}),
  abort: () => invoke<void>('abort'),

  renderToVideo: (
    projectId: string,
    compositionId: string,
    opts?: {quality?: string; target?: string},
  ) =>
    invoke<string>('render_to_video', {
      args: {
        project_id: projectId,
        composition_id: compositionId,
        quality: opts?.quality,
        target: opts?.target,
      },
    }),
  listRenders: () => invoke<RenderJob[]>('list_renders'),
  cancelRender: (jobId: string) => invoke<boolean>('cancel_render', {jobId}),

  getTimeline: (projectId: string, compositionId: string) =>
    invoke<TimelineState>('get_timeline', {projectId, compositionId}),
  snapshot: (projectId: string, tMs: number) =>
    invoke<SnapshotResult>('snapshot', {projectId, tMs}),
  startPreview: () => invoke<void>('preview_start'),
  stopPreview: () => invoke<void>('preview_stop'),
  previewStatus: () => invoke<PreviewStatus>('preview_status'),

  getSidecarStatus: () => invoke<SidecarHealth[]>('get_sidecar_status'),
  revealInFolder: (path: string) => invoke<void>('reveal_in_folder', {path}),
  packageFeedback: () => invoke<string>('package_feedback'),

  generateImage: (prompt: string, model?: string, size?: string) =>
    invoke<{
      source: string
      url: string | null
      revised_prompt: string | null
      error: string | null
    }>('generate_image', {args: {prompt, model, size}}),
  listCloudModels: () => invoke<{id: string; kind: string}[]>('list_cloud_models'),
  detectEngine: () => invoke<{id: string; name: string}[]>('detect_engine'),
  approve: (
    requestId: string,
    kind: string,
    payload: unknown,
    approved: boolean,
    alwaysAllow: boolean,
    value?: string,
  ) =>
    invoke<void>('approve', {
      requestId,
      kind,
      payload,
      approved,
      alwaysAllow,
      value: value ?? null,
    }),
  /** Save a composer attachment (base64 bytes → project assets dir). */
  saveAttachment: (name: string, dataB64: string, kind: 'image' | 'file') =>
    invoke<AttachmentInfo>('save_attachment', {name, dataB64, kind}),
  /** MCP server + runtime status for the Tools view. */
  getMcpStatus: () => invoke<McpStatus>('get_mcp_status'),
}

/** An asset pinned to the project (image / snapshot / generated media). */
export interface Asset {
  id: string
  project_id: string
  composition_id: string | null
  path: string
  kind: string // image | audio | video | ...
  source: string // cloud | local
  prompt: string | null
  created_at_ms: number
}
