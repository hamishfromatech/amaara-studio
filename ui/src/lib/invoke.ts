/**
 * Tauri invoke wrappers (Phase 6).
 *
 * Typed invoke() functions for every #[tauri::command]. The UI uses these to
 * communicate with the Rust core (config load/save, project operations, etc.).
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export async function invoke<T>(command: string, payload?: any): Promise<T> {
  return tauriInvoke(command, payload) as Promise<T>;
}

// Placeholder commands — wired in later phases.
export const Commands = {
  sendPrompt: (msg: string, mode: string) => invoke<void>("send_prompt", { msg, mode }),
  steer: (msg: string) => invoke<void>("steer", { msg }),
  abort: () => invoke<void>("abort"),
  setModel: (model: string) => invoke<void>("set_model", { model }),
  setSource: (source: string) => invoke<void>("set_source", { source }),
  setHarness: (harness: string) => invoke<void>("set_harness", { harness }),
  openProject: (dir: string) => invoke<void>("open_project", { dir }),
  newProject: (name: string) => invoke<void>("new_project", { name }),
  revealInFolder: (path: string) => invoke<void>("reveal_in_folder", { path }),
  getState: () => invoke<any>("get_state"),
};
