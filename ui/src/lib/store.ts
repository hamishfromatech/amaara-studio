/**
 * Global app store (Zustand) — the real reactive state for the studio shell.
 *
 * On mount the app calls `loadState()` which fetches the full snapshot from the
 * Rust core and subscribes to `studio://event`. Every command mutates state
 * through the store so the UI re-renders from real data, not hardcoded mocks.
 */

import { create } from "zustand";
import {
  Commands,
  type StateSnapshot,
  type RenderJob,
  type ProjectRow,
  type TimelineState,
  type Asset,
} from "./invoke";
import {
  subscribeStudioEvents,
  type StudioEvent,
  type StudioError,
  type HarnessEvent,
  type PreviewEvent,
} from "./events";
import type { NavId } from "./nav";

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

  /** Most recent typed studio error (Phase 15). Carries a suggested user
   * action; shown in the status strip with an action button. */
  studioError: StudioError | null;

  /** Live HyperFrames preview server (Timeline tab). */
  preview: {
    status: "idle" | "starting" | "running" | "error";
    url: string | null;
    port: number | null;
    error: string | null;
  };
  /** Parsed timeline for the current composition (Timeline tab). */
  timeline: TimelineState | null;
  /** Id of the clip currently selected in the inspector / playhead. */
  selectedClipId: string | null;
  /** Assets pinned to the current project (images, snapshots, media). */
  assets: Asset[];
  /** Per-sidecar log lines for the status-strip log drawer (Phase 15). */
  sidecarLogs: Record<string, SidecarLogLine[]>;

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
  setDensity: (density: "comfortable" | "compact") => Promise<void>;
  setShareAnalytics: (share: boolean) => Promise<void>;

  sendPrompt: (msg: string, mode?: string) => Promise<void>;
  steer: (msg: string) => Promise<void>;
  abort: () => Promise<void>;
  render: (quality?: string) => Promise<void>;
  cancelRender: (jobId: string) => Promise<void>;
  approve: (answer: "allow" | "deny" | "edit", alwaysAllow: boolean) => Promise<void>;

  // --- Preview (Timeline tab) --------------------------------------------
  startPreview: () => Promise<void>;
  stopPreview: () => Promise<void>;
  loadTimeline: (projectId: string, compositionId: string) => Promise<void>;
  selectClip: (clipId: string | null) => void;
  dismissError: () => void;

  // --- Layout / keyboard shortcuts (Phase 16) ----------------------------
  /** Active center pane (single source of truth; the global shortcuts hook
   *  reads this to scope tab- and timeline-specific hotkeys). */
  navId: NavId;
  setNavId: (id: NavId) => void;
  /** Which rails are collapsed (⌘\ left, ⌘/ right). */
  railsHidden: { left: boolean; right: boolean };
  toggleLeftRail: () => void;
  toggleRightRail: () => void;
  // --- Timeline transport (Phase 16) -------------------------------------
  // Lifted out of TimelineTab so the global hotkeys (Space / J / K / L) can
  // drive the playhead without TimelineTab owning the only copy of state.
  timelineTransport: {
    playheadMs: number;
    playing: boolean;
  };
  setPlayheadMs: (ms: number) => void;
  togglePlay: () => void;
  shuttle: (deltaMs: number) => void;

  /** Command palette (⌘K) open flag. */
  paletteOpen: boolean;
  setPaletteOpen: (open: boolean) => void;
  /** Keyboard-shortcuts help overlay (?) open flag. */
  shortcutsOpen: boolean;
  setShortcutsOpen: (open: boolean) => void;
}

let unsubEvents: (() => void) | null = null;

/** Max log lines kept per sidecar in the status-strip log drawer. */
const MAX_SIDECAR_LOG_LINES = 200;

/** One line in a sidecar's log drawer (status strip, Phase 15). */
export interface SidecarLogLine {
  ts: number;
  level: string;
  message: string;
}

/** Apply the theme to the document root (drives the CSS data-theme tokens). */
function applyTheme(theme: string) {
  const root = document.documentElement;
  root.setAttribute("data-theme", theme === "light" ? "light" : "dark");
}

/** Apply the density to the document root (drives the data-density tokens). */
function applyDensity(density: string) {
  const root = document.documentElement;
  root.setAttribute("data-density", density === "compact" ? "compact" : "comfortable");
}

export const useStore = create<AppState>((set, get) => ({
  loading: true,
  error: null,
  studioError: null,
  chat: [],
  renders: [],
  initialized: false,
  pendingApproval: null,

  preview: { status: "idle", url: null, port: null, error: null },
  timeline: null,
  selectedClipId: null,
  assets: [],
  sidecarLogs: {},

  // Layout / keyboard shortcuts (Phase 16).
  navId: "home",
  setNavId: (id) => set({ navId: id }),
  railsHidden: { left: false, right: false },
  toggleLeftRail: () =>
    set((s) => ({ railsHidden: { ...s.railsHidden, left: !s.railsHidden.left } })),
  toggleRightRail: () =>
    set((s) => ({ railsHidden: { ...s.railsHidden, right: !s.railsHidden.right } })),
  paletteOpen: false,
  setPaletteOpen: (open) => set({ paletteOpen: open }),
  shortcutsOpen: false,
  setShortcutsOpen: (open) => set({ shortcutsOpen: open }),

  timelineTransport: { playheadMs: 0, playing: false },
  setPlayheadMs: (ms) => set((s) => ({ timelineTransport: { ...s.timelineTransport, playheadMs: ms } })),
  togglePlay: () =>
    set((s) => ({ timelineTransport: { ...s.timelineTransport, playing: !s.timelineTransport.playing } })),
  // Move the playhead by deltaMs, clamped to [0, duration]. duration is read
  // lazily so a negative delta never needs the composition length to be known.
  shuttle: (deltaMs) =>
    set((s) => {
      const next = Math.max(0, Math.min(s.timelineTransport.playheadMs + deltaMs, s.timeline?.duration_ms ?? Infinity));
      return { timelineTransport: { ...s.timelineTransport, playheadMs: next } };
    }),

  loadState: async () => {
    try {
      const snap = await Commands.getState();
      set({ ...snap, loading: false, initialized: true, error: null });
      // Apply the persisted theme to the DOM (light/dark tokens).
      if (snap.config?.theme) applyTheme(snap.config.theme);
      if (snap.config?.density) applyDensity(snap.config.density);
      // Refresh engine models (discovered via the Navya Engine proxy).
      void get().refreshModels();
      // Subscribe once to the event stream.
      if (!unsubEvents) {
        const un = await subscribeStudioEvents(
          createEventCoalescer((ev) => handleEvent(ev, set, get))
        );
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
    // The composition changes with the project — tear down any preview.
    void get().stopPreview();
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

  setDensity: async (density) => {
    const config = get().config;
    if (!config) return;
    applyDensity(density); // apply immediately for responsiveness
    try {
      const saved = await Commands.saveConfig({ ...config, density });
      set({ config: saved });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setShareAnalytics: async (share) => {
    const config = get().config;
    if (!config) return;
    try {
      // Opt-in analytics: when off (default) the studio passes HyperFrames'
      // `--no-telemetry` to every render (Phase 15). Persist the flag so it
      // survives relaunch.
      const saved = await Commands.saveConfig({ ...config, share_analytics: share });
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

  // --- Preview (Timeline tab) --------------------------------------------
  // Start the live preview server for the current composition. Idempotent:
  // a running server is left alone; a failed one is retried.
  startPreview: async () => {
    const { preview } = get();
    if (preview.status === "running" || preview.status === "starting") return;
    set({ preview: { ...preview, status: "starting", error: null } });
    try {
      await Commands.startPreview();
    } catch (e) {
      set({ preview: { status: "error", url: null, port: null, error: String(e) } });
    }
  },

  // Stop the preview server (called when leaving the Timeline tab).
  stopPreview: async () => {
    try {
      await Commands.stopPreview();
    } catch {
      /* non-fatal — the Stopped event still clears state */
    }
  },

  // Parse the current composition into a timeline and select its first clip.
  loadTimeline: async (projectId, compositionId) => {
    try {
      const timeline = await Commands.getTimeline(projectId, compositionId);
      const selectedClipId = timeline.clips[0]?.id ?? null;
      set({ timeline, selectedClipId });
    } catch (e) {
      set({ timeline: null, selectedClipId: null, error: String(e) });
    }
  },

  selectClip: (clipId) => set({ selectedClipId: clipId }),

  // Dismiss the most recent typed studio error (Phase 15).
  dismissError: () => set({ studioError: null }),
}));

/**
 * Coalesce high-frequency events (open-design lesson B1): buffer incoming
 * events and flush every ~50ms (design.md's token-streaming budget), merging
 * runs of adjacent TextDelta events into one so the store re-renders at most
 * ~20x/s per stream instead of per token. Buffer order is FIFO — merging only
 * collapses adjacent deltas, so all other event semantics are preserved.
 */
const EVENT_BATCH_MS = 50;
function createEventCoalescer(sink: (ev: StudioEvent) => void): (ev: StudioEvent) => void {
  let buffer: StudioEvent[] = [];
  let timer: ReturnType<typeof setTimeout> | null = null;
  const flush = () => {
    timer = null;
    const batch = buffer;
    buffer = [];
    let i = 0;
    while (i < batch.length) {
      const ev = batch[i];
      if (ev.type === "Harness" && Object.keys(ev.payload)[0] === "TextDelta") {
        // Merge the run of adjacent TextDelta events into one.
        let text = String((ev.payload as Record<string, unknown>).TextDelta ?? "");
        i++;
        while (
          i < batch.length &&
          ev.type === "Harness" &&
          batch[i].type === "Harness" &&
          Object.keys(batch[i].payload)[0] === "TextDelta"
        ) {
          text += String((batch[i].payload as Record<string, unknown>).TextDelta ?? "");
          i++;
        }
        sink({ type: "Harness", payload: { TextDelta: text } } as StudioEvent);
        continue;
      }
      sink(ev);
      i++;
    }
  };
  return (ev: StudioEvent) => {
    buffer.push(ev);
    if (timer === null) {
      timer = setTimeout(flush, EVENT_BATCH_MS);
    }
  };
}

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
    const payload = ev.payload as { type: string; AssetAdded?: { project_id: string; asset_id: string; path: string } };
    if (payload.type === "AssetAdded" && payload.AssetAdded) {
      handleAssetAdded(payload.AssetAdded, set);
    } else {
      // Refresh the project list on create/switch.
      void useStore.getState().refreshProjects();
    }
  } else if (ev.type === "Sidecar") {
    handleSidecarEvent(ev.payload, set);
  } else if (ev.type === "Preview") {
    handlePreviewEvent(ev.payload, set);
  } else if (ev.type === "Error") {
    // Most recent typed studio error (Phase 15). Shows in the status strip
    // with a concrete user-action button; never a silent hang.
    set(() => ({ studioError: ev.payload as StudioError }));
  }
}

/** Accumulate an asset pinned via the snapshot command (AssetAdded event). */
function handleAssetAdded(
  a: { project_id: string; asset_id: string; path: string },
  set: (fn: (s: AppState) => Partial<AppState>) => void,
) {
  set((s) => {
    if (s.assets.some((x) => x.id === a.asset_id)) return {};
    const asset: Asset = {
      id: a.asset_id,
      project_id: a.project_id,
      composition_id: null,
      path: a.path,
      kind: "image",
      source: "local",
      prompt: null,
      created_at_ms: Date.now(),
    };
    return { assets: [...s.assets, asset] };
  });
}

function handlePreviewEvent(
  ev: PreviewEvent,
  set: (fn: (s: AppState) => Partial<AppState>) => void,
) {
  const kind = Object.keys(ev)[0] as keyof PreviewEvent;
  const p = (ev as Record<string, unknown>)[kind] as { url?: string; port?: number; error?: string };
  switch (kind) {
    case "Started":
      set(() => ({ preview: { status: "running", url: p.url ?? null, port: p.port ?? null, error: null } }));
      break;
    case "Stopped":
      set(() => ({ preview: { status: "idle", url: null, port: null, error: null } }));
      break;
    case "Failed":
      set(() => ({ preview: { status: "error", url: null, port: null, error: p.error ?? "preview failed to start" } }));
      break;
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
    // Every sidecar event seeds a line in the log drawer so lifecycle
    // transitions stay visible even when the process itself is quiet.
    const seed: SidecarLogLine | null =
      kind === "LogLine"
        ? { ts: Date.now(), level: String(p.level ?? "info"), message: String(p.message ?? "") }
        : kind === "Starting"
          ? { ts: Date.now(), level: "info", message: "starting…" }
          : kind === "Ready"
            ? { ts: Date.now(), level: "ok", message: "ready" }
            : kind === "Exit"
              ? { ts: Date.now(), level: "error", message: `exited with code ${p.code ?? "?"}` }
              : null;
    const prevLogs = s.sidecarLogs?.[name] ?? [];
    const sidecarLogs =
      seed !== null
        ? { ...s.sidecarLogs, [name]: [...prevLogs, seed].slice(-MAX_SIDECAR_LOG_LINES) }
        : s.sidecarLogs;
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
    return { sidecars, sidecarLogs };
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