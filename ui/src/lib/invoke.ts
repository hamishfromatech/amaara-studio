/**
 * Typed Tauri invoke wrappers — the real UI ↔ Rust bridge.
 *
 * Each function maps to a #[tauri::command] registered in src-tauri/src/lib.rs.
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export async function invoke<T>(command: string, payload?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(command, payload) as Promise<T>;
}

// --- Types mirrored from Rust -----------------------------------------------

export interface Session {
  current_project_id: string | null;
  current_composition_id: string | null;
  harness: string;
  model: string;
  source: string;
}

export interface NavyaConfig {
  navya_base_url: string;
  default_model: string;
  use_auto_router: boolean;
  byok: boolean;
  enabled_harnesses: string[];
  local_llama_url: string;
  sd_server_url: string | null;
  sd_binary_path: string;
  sd_models_dir: string;
  sd_gpu_backend: "cuda" | "vulkan" | "cpu";
  density: "comfortable" | "compact";
  theme: "dark" | "light";
}

export interface ModelEntry {
  id: string;
  name: string;
  kind: string; // chat | image | video
  source: string; // cloud | local
  active: boolean;
}

export interface HarnessInfo {
  id: string;
  label: string;
  enabled: boolean;
  steer: boolean;
  abort: boolean;
  persistent: boolean;
  available: boolean;
}

export interface SidecarHealth {
  name: string;
  status: string;
  detail: string | null;
}

export interface ProjectRow {
  id: string;
  name: string;
  dir: string;
  created_at_ms: number;
  harness: string;
  model: string;
  source: string;
}

export interface StateSnapshot {
  session: Session;
  config: NavyaConfig;
  projects: ProjectRow[];
  models: ModelEntry[];
  sidecars: SidecarHealth[];
  has_api_key: boolean;
  harnesses: HarnessInfo[];
  render_count: number;
}

export interface RenderJob {
  job_id: string;
  project_id: string;
  composition_id: string;
  target: string;
  quality: string;
  status: "queued" | "running" | "done" | "failed" | "cancelled";
  started_at_ms: number | null;
  finished_at_ms: number | null;
  output_path: string | null;
  error: string | null;
}

// --- Commands ---------------------------------------------------------------

export const Commands = {
  getState: () => invoke<StateSnapshot>("get_state"),
  getConfig: () => invoke<NavyaConfig>("get_config"),
  saveConfig: (config: NavyaConfig) => invoke<NavyaConfig>("save_config", { config }),
  setApiKey: (key: string) => invoke<void>("set_api_key", { key }),
  clearApiKey: () => invoke<void>("clear_api_key"),
  hasApiKey: () => invoke<boolean>("has_api_key"),

  newProject: (name: string, dir: string) =>
    invoke<ProjectRow>("new_project", { args: { name, dir } }),
  listProjects: () => invoke<ProjectRow[]>("list_projects"),
  openProject: (projectId: string) => invoke<void>("open_project", { projectId }),

  setModel: (model: string) => invoke<Session>("set_model", { model }),
  setSource: (source: string) => invoke<Session>("set_source", { source }),
  setHarness: (harness: string) => invoke<Session>("set_harness", { harness }),
  listModels: () => invoke<ModelEntry[]>("list_models"),

  sendPrompt: (msg: string, mode: string) => invoke<void>("send_prompt", { msg, mode }),
  steer: (msg: string) => invoke<void>("steer", { msg }),
  abort: () => invoke<void>("abort"),

  renderToVideo: (
    projectId: string,
    compositionId: string,
    opts?: { quality?: string; target?: string }
  ) =>
    invoke<string>("render_to_video", {
      args: {
        project_id: projectId,
        composition_id: compositionId,
        quality: opts?.quality,
        target: opts?.target,
      },
    }),
  listRenders: () => invoke<RenderJob[]>("list_renders"),
  cancelRender: (jobId: string) => invoke<boolean>("cancel_render", { jobId }),

  getSidecarStatus: () => invoke<SidecarHealth[]>("get_sidecar_status"),
  revealInFolder: (path: string) => invoke<void>("reveal_in_folder", { path }),

  generateImage: (prompt: string, model?: string, size?: string) => invoke<{ source: string; url: string | null; revised_prompt: string | null; error: string | null }>("generate_image", { args: { prompt, model, size } }),
  listCloudModels: () => invoke<{ id: string; kind: string }[]>("list_cloud_models"),
};