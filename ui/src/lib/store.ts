/**
 * Global app store (Zustand) — the real reactive state for the studio shell.
 *
 * On mount the app calls `loadState()` which fetches the full snapshot from the
 * Rust core and subscribes to `studio://event`. Every command mutates state
 * through the store so the UI re-renders from real data, not hardcoded mocks.
 */

import { create } from "zustand";
import { Commands, type StateSnapshot, type RenderJob, type ProjectRow } from "./invoke";
import { subscribeStudioEvents, type StudioEvent, type HarnessEvent } from "./events";

interface ChatMessage {
  id: string;
  role: "you" | "agent" | "system";
  content: string;
  status?: "thinking" | "tool-calling" | "done" | "error";
}

interface AppState extends Partial<StateSnapshot> {
  loading: boolean;
  error: string | null;
  chat: ChatMessage[];
  renders: RenderJob[];
  initialized: boolean;
  /** Pending approval request from the harness (shown as a modal). */
  pendingApproval: { id: string; kind: string; payload: unknown } | null;

  loadState: () => Promise<void>;
  refreshProjects: () => Promise<void>;
  refreshRenders: () => Promise<void>;
  refreshModels: () => Promise<void>;

  setModel: (model: string) => Promise<void>;
  setSource: (source: string) => Promise<void>;
  setHarness: (harness: string) => Promise<void>;

  newProject: (name: string, dir: string) => Promise<void>;
  openProject: (projectId: string) => Promise<void>;

  saveApiKey: (key: string) => Promise<void>;
  clearApiKey: () => Promise<void>;
  saveConfig: (config: NonNullable<StateSnapshot["config"]>) => Promise<void>;
  setTheme: (theme: "dark" | "light") => Promise<void>;

  sendPrompt: (msg: string, mode?: string) => Promise<void>;
  steer: (msg: string) => Promise<void>;
  abort: () => Promise<void>;
  render: (quality?: string) => Promise<void>;
  cancelRender: (jobId: string) => Promise<void>;
  approve: (answer: "allow" | "deny" | "edit", alwaysAllow: boolean) => Promise<void>;
}

let unsubEvents: (() => void) | null = null;

/** Apply the theme to the document root (drives the CSS data-theme tokens). */
function applyTheme(theme: string) {
  const root = document.documentElement;
  root.setAttribute("data-theme", theme === "light" ? "light" : "dark");
}

export const useStore = create<AppState>((set, get) => ({
  loading: true,
  error: null,
  chat: [],
  renders: [],
  initialized: false,
  pendingApproval: null,

  loadState: async () => {
    try {
      const snap = await Commands.getState();
      set({ ...snap, loading: false, initialized: true, error: null });
      // Apply the persisted theme to the DOM (light/dark tokens).
      if (snap.config?.theme) applyTheme(snap.config.theme);
      // Refresh engine models (discovered via the Navya Engine proxy).
      void get().refreshModels();
      // Subscribe once to the event stream.
      if (!unsubEvents) {
        const un = await subscribeStudioEvents((ev) => handleEvent(ev, set, get));
        unsubEvents = un;
      }
    } catch (e) {
      set({ loading: false, error: String(e), initialized: true });
    }
  },

  refreshProjects: async () => {
    try {
      const projects = await Commands.listProjects();
      set({ projects });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  refreshRenders: async () => {
    try {
      const renders = await Commands.listRenders();
      set({ renders });
    } catch (e) {
      /* non-fatal */
    }
  },

  refreshModels: async () => {
    try {
      const models = await Commands.listModels();
      set({ models });
    } catch (e) {
      /* non-fatal */
    }
  },

  setModel: async (model) => {
    const session = await Commands.setModel(model);
    set((s) => ({ session, models: (s.models ?? []).map((m) => ({ ...m, active: m.id === model })) }));
  },

  setSource: async (source) => {
    const session = await Commands.setSource(source);
    set({ session });
  },

  setHarness: async (harness) => {
    const session = await Commands.setHarness(harness);
    set({ session });
  },

  newProject: async (name, dir) => {
    await Commands.newProject(name, dir);
    await get().refreshProjects();
  },

  openProject: async (projectId) => {
    await Commands.openProject(projectId);
    const snap = await Commands.getState();
    set({ session: snap.session });
  },

  saveApiKey: async (key) => {
    await Commands.setApiKey(key);
    set({ has_api_key: true });
  },

  clearApiKey: async () => {
    await Commands.clearApiKey();
    set({ has_api_key: false });
  },

  saveConfig: async (config) => {
    const saved = await Commands.saveConfig(config);
    set({ config: saved });
  },

  setTheme: async (theme) => {
    const config = get().config;
    if (!config) return;
    applyTheme(theme); // apply immediately for responsiveness
    try {
      const saved = await Commands.saveConfig({ ...config, theme });
      set({ config: saved });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  sendPrompt: async (msg, mode = "normal") => {
    set((s) => ({
      chat: [
        ...s.chat,
        { id: `u${Date.now()}`, role: "you", content: msg },
        { id: `a${Date.now()}`, role: "agent", content: "", status: "thinking" },
      ],
    }));
    try {
      await Commands.sendPrompt(msg, mode);
    } catch (e) {
      set((s) => ({
        chat: s.chat.map((m) =>
          m.role === "agent" && m.status === "thinking"
            ? { ...m, status: "error", content: String(e) }
            : m
        ),
        error: String(e),
      }));
    }
  },

  steer: async (msg) => {
    try {
      await Commands.steer(msg);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  abort: async () => {
    try {
      await Commands.abort();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  render: async (quality = "draft") => {
    const session = get().session;
    if (!session?.current_project_id) {
      set({ error: "Open or create a project before rendering." });
      return;
    }
    try {
      await Commands.renderToVideo(
        session.current_project_id,
        session.current_composition_id ?? "main",
        { quality }
      );
      await get().refreshRenders();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  cancelRender: async (jobId) => {
    await Commands.cancelRender(jobId);
    await get().refreshRenders();
  },

  approve: async (answer, alwaysAllow) => {
    const req = get().pendingApproval;
    if (!req) return;
    const approved = answer === "allow";
    set({ pendingApproval: null });
    try {
      await Commands.approve(req.id, req.kind, req.payload, approved, alwaysAllow);
    } catch (e) {
      set({ error: String(e) });
    }
  },
}));

/** Reduce a StudioEvent into store mutations. */
function handleEvent(
  ev: StudioEvent,
  set: (fn: (s: AppState) => Partial<AppState>) => void,
  _get: () => AppState
) {
  if (ev.type === "Harness") {
    handleHarnessEvent(ev.payload, set);
  } else if (ev.type === "Render") {
    handleRenderEvent(ev.payload, set);
  } else if (ev.type === "Project") {
    // Refresh the project list on create/switch.
    void useStore.getState().refreshProjects();
  } else if (ev.type === "Sidecar") {
    handleSidecarEvent(ev.payload, set);
  }
}

function handleSidecarEvent(
  ev: Record<string, unknown>,
  set: (fn: (s: AppState) => Partial<AppState>) => void
) {
  const kind = Object.keys(ev)[0];
  const p = (ev as Record<string, unknown>)[kind] as Record<string, unknown>;
  const name = String(p.name ?? "");
  set((s) => {
    const sidecars = (s.sidecars ?? []).map((sc) => {
      if (sc.name !== name) return sc;
      if (kind === "Starting") return { ...sc, status: "starting" };
      if (kind === "Ready") return { ...sc, status: "running" };
      if (kind === "Exit") {
        const code = Number(p.code ?? -1);
        return { ...sc, status: "exited", detail: `exit code ${code}` };
      }
      return sc;
    });
    return { sidecars };
  });
}

function handleHarnessEvent(
  ev: HarnessEvent,
  set: (fn: (s: AppState) => Partial<AppState>) => void
) {
  const kind = Object.keys(ev)[0] as keyof HarnessEvent;
  const payload = (ev as Record<string, unknown>)[kind];

  set((s) => {
    const chat = [...s.chat];
    const lastAgent = [...chat].reverse().find((m) => m.role === "agent");
    switch (kind) {
      case "ApprovalRequest": {
        const p = payload as { id: string; kind: string; payload: unknown };
        // Surface the approval modal; keep chat untouched.
        return { pendingApproval: { id: p.id, kind: p.kind, payload: p.payload } };
      }
      case "TextDelta": {
        if (lastAgent) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id);
          if (idx >= 0) {
            chat[idx] = { ...chat[idx], content: chat[idx].content + String(payload), status: "tool-calling" };
          }
        }
        break;
      }
      case "ToolStart": {
        const p = payload as { name: string };
        if (lastAgent) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id);
          if (idx >= 0) {
            const existing = chat[idx].content;
            chat[idx] = {
              ...chat[idx],
              status: "tool-calling",
              content: existing + (existing ? "\n" : "") + `\u2699 ${p.name} …`,
            };
          }
        }
        break;
      }
      case "ToolEnd": {
        const p = payload as { is_error: boolean; result: string | null };
        if (lastAgent && p.is_error) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id);
          if (idx >= 0) {
            const existing = chat[idx].content;
            chat[idx] = {
              ...chat[idx],
              content: existing + `\n\u2716 tool failed${p.result ? `: ${p.result}` : ""}`,
            };
          }
        }
        break;
      }
      case "AgentEnd": {
        const p = payload as { success: boolean; message: string | null };
        if (lastAgent) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id);
          if (idx >= 0) chat[idx] = { ...chat[idx], status: p.success ? "done" : "error" };
        }
        break;
      }
      case "Error": {
        if (lastAgent) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id);
          if (idx >= 0) chat[idx] = { ...chat[idx], status: "error", content: String(payload) };
        } else {
          chat.push({ id: `e${Date.now()}`, role: "system", content: String(payload) });
        }
        break;
      }
      case "AgentStart": {
        // mark the trailing agent message as thinking
        if (lastAgent && lastAgent.status === undefined) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id);
          if (idx >= 0) chat[idx] = { ...chat[idx], status: "thinking" };
        }
        break;
      }
      default:
        break;
    }
    return { chat };
  });
}

function handleRenderEvent(
  ev: StudioEvent["payload"] extends infer R ? Extract<StudioEvent, { type: "Render" }>["payload"] : never,
  set: (fn: (s: AppState) => Partial<AppState>) => void
) {
  const kind = Object.keys(ev)[0];
  const p = (ev as Record<string, unknown>)[kind] as Record<string, unknown>;
  const jobId = String(p.job_id);
  set((s) => {
    const renders = [...s.renders];
    const idx = renders.findIndex((r) => r.job_id === jobId);
    if (kind === "Started") {
      if (idx < 0) {
        renders.push({
          job_id: jobId,
          project_id: "",
          composition_id: "",
          target: String(p.target),
          quality: String(p.quality),
          status: "running",
          started_at_ms: Date.now(),
          finished_at_ms: null,
          output_path: null,
          error: null,
        });
      }
    } else if (kind === "Completed") {
      if (idx >= 0) {
        renders[idx] = { ...renders[idx], status: "done", output_path: String(p.output_path), finished_at_ms: Date.now(), progress: null };
      }
    } else if (kind === "Progress") {
      const progress = {
        stage: String(p.stage ?? ""),
        frame: Number(p.frame ?? 0),
        total_frames: p.total_frames == null ? null : Number(p.total_frames),
      };
      if (idx >= 0) {
        renders[idx] = { ...renders[idx], progress };
      } else {
        renders.push({
          job_id: jobId,
          project_id: "",
          composition_id: "",
          target: "",
          quality: "",
          status: "running",
          started_at_ms: Date.now(),
          finished_at_ms: null,
          output_path: null,
          error: null,
          progress,
        });
      }
    } else if (kind === "Failed") {
      if (idx >= 0) {
        renders[idx] = { ...renders[idx], status: "failed", error: String(p.error), finished_at_ms: Date.now() };
      }
    } else if (kind === "Cancelled") {
      if (idx >= 0) renders[idx] = { ...renders[idx], status: "cancelled", finished_at_ms: Date.now() };
    }
    return { renders };
  });
}