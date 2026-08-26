/**
 * Studio event listener (production wiring).
 *
 * Listens to the single `studio://event` channel from the Rust core and
 * dispatches typed StudioEvents to subscribers. The UI uses this for live
 * updates: harness text/tool deltas, render progress, sidecar status, project
 * switches, and errors.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Mirror of src-tauri/src/events.rs StudioEvent (externally-tagged enum).
export type StudioEvent =
  | { type: "Harness"; payload: HarnessEvent }
  | { type: "Render"; payload: RenderEvent }
  | { type: "Sidecar"; payload: SidecarEvent }
  | { type: "Project"; payload: ProjectEvent };

export type HarnessEvent =
  | { AgentStart: { model: string } }
  | { AgentEnd: { success: boolean; message: string | null } }
  | { TextDelta: string }
  | { ThinkingDelta: string }
  | { ToolStart: { tool_id: string; name: string; args: unknown } }
  | { ToolUpdate: { tool_id: string; partial: string } }
  | { ToolEnd: { tool_id: string; result: string | null; is_error: boolean } }
  | { ApprovalRequest: { id: string; kind: string; payload: unknown } }
  | { QueueUpdate: { steer: boolean; follow_up: boolean } }
  | { Retry: { attempt: number; reason: string } }
  | { Error: string };

export type RenderEvent =
  | { Started: { job_id: string; target: string; quality: string } }
  | { Progress: { job_id: string; stage: string; frame: number; total_frames: number | null } }
  | { Completed: { job_id: string; output_path: string; duration_ms: number | null } }
  | { Failed: { job_id: string; error: string } }
  | { Cancelled: { job_id: string } };

export type SidecarEvent =
  | { Starting: { name: string } }
  | { Ready: { name: string } }
  | { Exit: { name: string; code: number } }
  | { LogLine: { name: string; level: string; message: string } };

export type ProjectEvent =
  | { Created: { project_id: string; name: string } }
  | { Switched: { project_id: string; name: string } }
  | { Updated: { project_id: string; changed_at: number } };

export type StudioEventListener = (event: StudioEvent) => void;

/**
 * Subscribe to the studio event channel. Returns an unsubscribe function.
 * Safe to call multiple times; each subscription gets its own listener.
 */
export async function subscribeStudioEvents(listener: StudioEventListener): Promise<UnlistenFn> {
  return listen<StudioEvent>("studio://event", (e) => {
    listener(e.payload);
  });
}

