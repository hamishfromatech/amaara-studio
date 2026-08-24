/**
 * Studio event listener and reducer (Phase 6).
 *
 * Listens to 'studio://event' from Rust core and reduces StudioEvents into UI state:
 * turns, tool cards, render queue, sidecar status dots.
 */

import { listen } from "@tauri-apps/api/event";

export type StudioEvent = {
  type: "harness" | "render" | "sidecar" | "project";
  payload: any;
};

// UI state shape for the chat/reducer.
export interface UiState {
  turns: Turn[];
  toolCards: ToolCard[];
  renderQueue: RenderJob[];
  sidecarStatus: Record<string, string>; // name -> "idle" | "busy" | "error"
}

export interface Turn {
  id: string;
  role: "you" | "agent";
  content: string;
  status: "thinking" | "tool-calling" | "done" | "error";
}

export interface ToolCard {
  id: string;
  type: "write" | "edit" | "generate_image" | "bash" | "render_to_video";
  name?: string;
  args?: any;
  result?: string;
  is_error: boolean;
}

export interface RenderJob {
  job_id: string;
  project_id: string;
  composition_id: string;
  target: string;
  quality: string;
  status: "queued" | "running" | "done" | "failed";
  progress?: number; // 0-100
}

// Initial state.
export const initialUiState: UiState = {
  turns: [],
  toolCards: [],
  renderQueue: [],
  sidecarStatus: {
    harness: "idle",
    render: "ready",
    "sd-server": "idle",
  },
};

// Event reducer — turns StudioEvent into UiState updates.
export function reduceUiState(state: UiState, event: StudioEvent): UiState {
  let next = { ...state };

  if (event.type === "harness") {
    const he = event.payload;
    // Handle tool cards and text deltas...
    if (he.toolStart) {
      next.toolCards.push({
        id: he.toolStart.tool_id,
        type: getCardType(he.toolStart.name),
        name: he.toolStart.name,
        args: he.toolStart.args,
        is_error: false,
      });
    }
    if (he.agentEnd) {
      // Mark last turn as done/error
    }
  }

  if (event.type === "render") {
    const re = event.payload;
    if (re.started) {
      next.renderQueue.push({
        job_id: re.started.job_id,
        project_id: re.started.project_id || "default",
        composition_id: re.started.composition_id || "default",
        target: re.started.target || "local",
        quality: re.started.quality || "draft",
        status: "running",
        progress: 0,
      });
    }
    if (re.progress) {
      // Update progress for existing job...
    }
  }

  if (event.type === "sidecar") {
    const se = event.payload;
    if (se.ready || se.starting) {
      next.sidecarStatus[se.name] = "busy";
    }
    if (se.exit || se.stopped) {
      next.sidecarStatus[se.name] = "idle";
    }
  }

  return next;
}

function getCardType(name: string): ToolCard["type"] {
  if (name === "write" || name === "edit") return name;
  if (name === "generate_image") return "generate_image";
  if (name === "bash" || name === "shell") return "bash";
  if (name === "render_to_video") return "render_to_video";
  return "bash"; // fallback
}

// Listen to studio events and reduce into state.
export function setupEventListeners(setState: (fn: (s: UiState) => UiState) => void) {
  listen("studio://event", (event: any) => {
    // Parse the event payload...
    const parsed: StudioEvent = {
      type: event.event || "harness",
      payload: event.payload,
    };
    // In a real app, use Zustand or Redux to dispatch reduceUiState.
    // For M0 scaffold, we just log or store in state ref.
    console.log("studio event:", parsed);
  });
}
