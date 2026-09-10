/**
 * Global app store (Zustand) — the real reactive state for the studio shell.
 *
 * On mount the app calls `loadState()` which fetches the full snapshot from the
 * Rust core and subscribes to `studio://event`. Every command mutates state
 * through the store so the UI re-renders from real data, not hardcoded mocks.
 */

import {create} from 'zustand'
import {
  Commands,
  type StateSnapshot,
  type RenderJob,
  type ProjectRow,
  type TimelineState,
  type Asset,
  type AttachmentInfo,
} from './invoke'

/** One structured tool call inside an agent turn (design.md §3 tool cards).
 *  Rendered as a live card: collapsed by default, expandable to args/output. */
export interface ToolCall {
  id: string
  name: string
  args: unknown
  /** Incremental stdout / partial output (ToolUpdate events). */
  partial: string
  result: string | null
  isError: boolean
  done: boolean
  startedAtMs: number
}

/** A composer attachment staged for the next send (design.md §3 attach). */
export interface ComposerAttachment {
  info: AttachmentInfo
  /** Object URL for image previews; null for non-image files. */
  previewUrl: string | null
}
import {
  subscribeStudioEvents,
  type StudioEvent,
  type StudioError,
  type HarnessEvent,
  type PreviewEvent,
  type RenderEvent,
} from './events'
import type {NavId} from './nav'

export interface ChatMessage {
  id: string
  role: 'you' | 'agent' | 'system'
  content: string
  status?: 'thinking' | 'tool-calling' | 'done' | 'error'
  /** Wall-clock start of the run — anchors the elapsed clock across remounts. */
  startedAtMs?: number
  /** Set on failed agent messages: the prompt to offer as a retry. */
  failedPrompt?: string
  /** Structured tool calls rendered as live cards (design.md §3). */
  tools: ToolCall[]
  /** How the user sent this message (⌘⇧Enter = steer, ⌘⌥Enter = follow-up).
   *  Rendered with a small glyph so steers/follow-ups are legible in chat. */
  mode?: 'normal' | 'steer' | 'follow_up'
}

interface AppState extends Partial<StateSnapshot> {
  loading: boolean
  error: string | null
  chat: ChatMessage[]
  renders: RenderJob[]
  initialized: boolean
  /** Pending approval request from the harness (shown as a modal). */
  pendingApproval: {id: string; kind: string; payload: unknown} | null

  /** Most recent typed studio error (Phase 15). Carries a suggested user
   * action; shown in the status strip with an action button. */
  studioError: StudioError | null

  /** Live HyperFrames preview server (Timeline tab). */
  preview: {
    status: 'idle' | 'starting' | 'running' | 'error'
    url: string | null
    port: number | null
    error: string | null
  }
  /** Parsed timeline for the current composition (Timeline tab). */
  timeline: TimelineState | null
  /** Id of the clip currently selected in the inspector / playhead. */
  selectedClipId: string | null
  /** Assets pinned to the current project (images, snapshots, media). */
  assets: Asset[]
  /** Per-sidecar log lines for the status-strip log drawer (Phase 15). */
  sidecarLogs: Record<string, SidecarLogLine[]>

  loadState: () => Promise<void>
  refreshProjects: () => Promise<void>
  refreshRenders: () => Promise<void>
  refreshModels: () => Promise<void>
  refreshSidecars: () => Promise<void>
  refreshProjectContext: () => Promise<void>

  setModel: (model: string) => Promise<void>
  setSource: (source: string) => Promise<void>
  setHarness: (harness: string) => Promise<void>

  newProject: (name: string, dir: string) => Promise<void>
  openProject: (projectId: string) => Promise<void>
  /** Delete a project (metadata only — files on disk stay). */
  deleteProject: (projectId: string) => Promise<void>

  saveApiKey: (key: string) => Promise<void>
  clearApiKey: () => Promise<void>
  saveConfig: (config: NonNullable<StateSnapshot['config']>) => Promise<void>
  setTheme: (theme: 'dark' | 'light') => Promise<void>
  setDensity: (density: 'comfortable' | 'compact') => Promise<void>
  setShareAnalytics: (share: boolean) => Promise<void>

  sendPrompt: (msg: string, mode?: 'normal' | 'follow_up') => Promise<void>
  /** Composer draft — shared between the Home and Chat surfaces so a switch
   *  never loses typed text. */
  draft: string
  setDraft: (text: string) => void
  /** Queued prompts typed while a run is in flight (open-design QueuedSendStrip).
   *  Each keeps its mode so a follow-up is re-sent as follow-up on turn end. */
  queuedPrompts: {text: string; mode: 'normal' | 'follow_up'}[]
  removeQueuedPrompt: (index: number) => void
  sendQueuedNow: (index: number) => void
  steer: (msg: string) => Promise<void>
  abort: () => Promise<void>
  render: (quality?: string, target?: string) => Promise<void>
  cancelRender: (jobId: string) => Promise<void>
  approve: (
    answer: 'allow' | 'deny' | 'edit',
    alwaysAllow: boolean,
    value?: string,
  ) => Promise<void>

  // --- Composer attachments (design.md §3 attach image / @file) -----------
  /** Files staged in the composer; referenced (project-relative) on send. */
  attachments: ComposerAttachment[]
  attachFiles: (files: FileList | File[]) => Promise<void>
  removeAttachment: (id: string) => void
  clearAttachments: () => void

  // --- Accessibility: live-region announcements ---------------------------
  /** Latest screen-reader announcement; `n` re-triggers repeats. */
  announcement: {text: string; n: number}
  announce: (text: string) => void

  // --- Render… dialog (⌘⇧R, design.md §10) -------------------------------
  renderDialogOpen: boolean
  setRenderDialogOpen: (open: boolean) => void

  // --- Preview (Timeline tab) --------------------------------------------
  startPreview: () => Promise<void>
  stopPreview: () => Promise<void>
  loadTimeline: (projectId: string, compositionId: string) => Promise<void>
  selectClip: (clipId: string | null) => void
  dismissError: () => void

  // --- Layout / keyboard shortcuts (Phase 16) ----------------------------
  /** Active center pane (single source of truth; the global shortcuts hook
   *  reads this to scope tab- and timeline-specific hotkeys). */
  navId: NavId
  setNavId: (id: NavId) => void
  /** Which rails are collapsed (⌘\ left, ⌘/ right). */
  railsHidden: {left: boolean; right: boolean}
  toggleLeftRail: () => void
  toggleRightRail: () => void
  // --- Timeline transport (Phase 16) -------------------------------------
  // Lifted out of TimelineTab so the global hotkeys (Space / J / K / L) can
  // drive the playhead without TimelineTab owning the only copy of state.
  timelineTransport: {
    playheadMs: number
    playing: boolean
  }
  setPlayheadMs: (ms: number) => void
  togglePlay: () => void
  shuttle: (deltaMs: number) => void

  /** Command palette (⌘K) open flag. */
  paletteOpen: boolean
  setPaletteOpen: (open: boolean) => void
  /** Keyboard-shortcuts help overlay (?) open flag. */
  shortcutsOpen: boolean
  setShortcutsOpen: (open: boolean) => void
}

let unsubEvents: (() => void) | null = null
/** Synchronous subscribe-in-progress flag. loadState can be invoked twice
 *  concurrently (React StrictMode double-mounts effects in dev); both calls
 *  used to see `unsubEvents === null` before either await resolved, so BOTH
 *  subscribed and every studio event was delivered twice. */
let subscribing = false

/** Max log lines kept per sidecar in the status-strip log drawer. */
const MAX_SIDECAR_LOG_LINES = 200

/** One line in a sidecar's log drawer (status strip, Phase 15). */
export interface SidecarLogLine {
  ts: number
  level: string
  message: string
}

/** Apply the theme to the document root (drives the CSS data-theme tokens). */
function applyTheme(theme: string) {
  const root = document.documentElement
  root.setAttribute('data-theme', theme === 'light' ? 'light' : 'dark')
}

/** Apply the density to the document root (drives the data-density tokens). */
function applyDensity(density: string) {
  const root = document.documentElement
  root.setAttribute('data-density', density === 'compact' ? 'compact' : 'comfortable')
}

export const useStore = create<AppState>((set, get) => ({
  loading: true,
  error: null,
  studioError: null,
  chat: [],
  renders: [],
  initialized: false,
  pendingApproval: null,
  attachments: [],

  preview: {status: 'idle', url: null, port: null, error: null},
  timeline: null,
  selectedClipId: null,
  assets: [],
  sidecarLogs: {},

  // Layout / keyboard shortcuts (Phase 16).
  navId: 'home',
  setNavId: (id) => set({navId: id}),
  announcement: {text: '', n: 0},
  announce: (text) => set((s) => ({announcement: {text, n: s.announcement.n + 1}})),
  renderDialogOpen: false,
  setRenderDialogOpen: (open) => set({renderDialogOpen: open}),
  railsHidden: {left: false, right: false},
  toggleLeftRail: () => set((s) => ({railsHidden: {...s.railsHidden, left: !s.railsHidden.left}})),
  toggleRightRail: () =>
    set((s) => ({railsHidden: {...s.railsHidden, right: !s.railsHidden.right}})),
  paletteOpen: false,
  setPaletteOpen: (open) => set({paletteOpen: open}),
  shortcutsOpen: false,
  setShortcutsOpen: (open) => set({shortcutsOpen: open}),

  timelineTransport: {playheadMs: 0, playing: false},
  setPlayheadMs: (ms) =>
    set((s) => ({timelineTransport: {...s.timelineTransport, playheadMs: ms}})),
  togglePlay: () =>
    set((s) => ({
      timelineTransport: {...s.timelineTransport, playing: !s.timelineTransport.playing},
    })),
  // Move the playhead by deltaMs, clamped to [0, duration]. duration is read
  // lazily so a negative delta never needs the composition length to be known.
  shuttle: (deltaMs) =>
    set((s) => {
      const next = Math.max(
        0,
        Math.min(s.timelineTransport.playheadMs + deltaMs, s.timeline?.duration_ms ?? Infinity),
      )
      return {timelineTransport: {...s.timelineTransport, playheadMs: next}}
    }),

  loadState: async () => {
    try {
      const snap = await Commands.getState()
      set({...snap, loading: false, initialized: true, error: null})
      // Apply the persisted theme to the DOM (light/dark tokens).
      if (snap.config?.theme) applyTheme(snap.config.theme)
      if (snap.config?.density) applyDensity(snap.config.density)
      // Refresh engine models (discovered via the Navya Engine proxy).
      void get().refreshModels()
      // Subscribe once to the event stream. The synchronous `subscribing`
      // flag closes the StrictMode race (see note above).
      if (!unsubEvents && !subscribing) {
        subscribing = true
        try {
          const un = await subscribeStudioEvents(
            createEventCoalescer((ev) => handleEvent(ev, set, get)),
          )
          unsubEvents = un
        } finally {
          subscribing = false
        }
      }
    } catch (e) {
      set({loading: false, error: String(e), initialized: true})
    }
  },

  refreshProjects: async () => {
    try {
      const projects = await Commands.listProjects()
      set({projects})
    } catch (e) {
      set({error: String(e)})
    }
  },

  refreshRenders: async () => {
    try {
      const renders = await Commands.listRenders()
      set({renders})
    } catch (e) {
      /* non-fatal */
    }
  },

  refreshProjectContext: async () => {
    try {
      const snap = await Commands.getState()
      // NOTE: models intentionally NOT copied from the snapshot — snapshot
      // models are the static fallback list, and copying them here used to
      // wipe the live harness/engine picker whenever a project event fired.
      // The dedicated refreshModels() below restores the live catalog.
      set({session: snap.session, projects: snap.projects})
      // Renders are scoped to the project — always re-read them on context
      // changes (create/switch/update).
      await get().refreshRenders()
      await get().refreshModels()
    } catch (e) {
      /* non-fatal */
    }
  },

  refreshModels: async () => {
    try {
      const models = await Commands.listModels()
      set({models})
    } catch (e) {
      /* non-fatal */
    }
  },

  refreshSidecars: async () => {
    try {
      const sidecars = await Commands.getSidecarStatus()
      set({sidecars})
    } catch (e) {
      /* non-fatal */
    }
  },

  setModel: async (model) => {
    try {
      const session = await Commands.setModel(model)
      set((s) => ({session, models: (s.models ?? []).map((m) => ({...m, active: m.id === model}))}))
      useStore.getState().announce(`Model set to ${model}`)
    } catch (e) {
      useStore.getState().announce('Could not set model')
      set({error: String(e)})
    }
  },

  setSource: async (source) => {
    try {
      const session = await Commands.setSource(source)
      set({session})
      useStore.getState().announce(`Generation source: ${source}`)
    } catch (e) {
      useStore.getState().announce('Could not switch source')
      set({error: String(e)})
    }
  },

  setHarness: async (harness) => {
    const session = await Commands.setHarness(harness)
    set({session})
    // The new harness has its own model catalog (live RPC for a-coder-cli,
    // static catalog for the others) — refresh the picker for it.
    await get().refreshModels()
    useStore.getState().announce(`Harness: ${harness}`)
  },

  newProject: async (name, dir) => {
    let row: ProjectRow
    try {
      row = await Commands.newProject(name, dir)
    } catch (e) {
      // Surface the failure (status strip + announcement), then rethrow so
      // callers can keep their form open with the user's input intact.
      useStore.getState().announce('Could not create project')
      set({error: String(e)})
      throw e
    }
    await get().refreshProjectContext()
    useStore.getState().announce(`Project ${row.name} created`)
  },

  openProject: async (projectId) => {
    // A running harness was spawned in the previous project's dir — stop it so
    // its stream doesn't leak into the new project's chat.
    const target = (useStore.getState().projects ?? []).find((p) => p.id === projectId)
    void get().abort()
    try {
      await Commands.openProject(projectId)
    } catch (e) {
      useStore.getState().announce('Could not open project')
      set({error: String(e)})
      return
    }
    // Chat, assets and the preview all belong to the project being opened —
    // start from a clean slate so the previous project's state doesn't linger.
    useStore.getState().clearAttachments()
    set({
      chat: [],
      assets: [],
      timeline: null,
      selectedClipId: null,
      pendingApproval: null,
      queuedPrompts: [],
    })
    await get().refreshProjectContext()
    useStore.getState().announce(target ? `Opened ${target.name}` : 'Project opened')
  },

  deleteProject: async (projectId) => {
    const wasActive = get().session?.current_project_id === projectId
    if (wasActive) {
      // Stop a harness running inside that project before removing it.
      void get().abort()
    }
    try {
      const deleted = await Commands.deleteProject(projectId)
      if (!deleted) {
        useStore.getState().announce('Project not found')
        return
      }
      if (wasActive) {
        set({
          chat: [],
          assets: [],
          timeline: null,
          selectedClipId: null,
          pendingApproval: null,
          queuedPrompts: [],
        })
      }
      useStore.getState().announce('Project deleted')
      await get().refreshProjectContext()
    } catch (e) {
      useStore.getState().announce('Could not delete project')
      set({error: String(e)})
    }
  },

  saveApiKey: async (key) => {
    try {
      await Commands.setApiKey(key)
      set({has_api_key: true})
      useStore.getState().announce('API key saved')
    } catch (e) {
      useStore.getState().announce('Could not save API key')
      set({error: String(e)})
    }
  },

  clearApiKey: async () => {
    try {
      await Commands.clearApiKey()
      set({has_api_key: false})
      useStore.getState().announce('API key cleared')
    } catch (e) {
      useStore.getState().announce('Could not clear API key')
      set({error: String(e)})
    }
  },

  saveConfig: async (config) => {
    try {
      const saved = await Commands.saveConfig(config)
      set({config: saved})
      useStore.getState().announce('Settings saved')
    } catch (e) {
      useStore.getState().announce('Could not save settings')
      set({error: String(e)})
    }
  },

  setTheme: async (theme) => {
    const config = get().config
    if (!config) return
    applyTheme(theme) // apply immediately for responsiveness
    try {
      const saved = await Commands.saveConfig({...config, theme})
      set({config: saved})
    } catch (e) {
      set({error: String(e)})
    }
  },

  setDensity: async (density) => {
    const config = get().config
    if (!config) return
    applyDensity(density) // apply immediately for responsiveness
    try {
      const saved = await Commands.saveConfig({...config, density})
      set({config: saved})
    } catch (e) {
      set({error: String(e)})
    }
  },

  setShareAnalytics: async (share) => {
    const config = get().config
    if (!config) return
    try {
      // Opt-in analytics: when off (default) the studio passes HyperFrames'
      // `--no-telemetry` to every render (Phase 15). Persist the flag so it
      // survives relaunch.
      const saved = await Commands.saveConfig({...config, share_analytics: share})
      set({config: saved})
    } catch (e) {
      set({error: String(e)})
    }
  },

  // Composer draft (shared Home/Chat) + queued prompts — flushed FIFO as
  // turns end (open-design QueuedSendStrip).
  draft: '',
  setDraft: (text) => set({draft: text}),
  queuedPrompts: [],
  removeQueuedPrompt: (index) =>
    set((s) => ({queuedPrompts: s.queuedPrompts.filter((_, i) => i !== index)})),
  sendQueuedNow: (index) => {
    const queue = get().queuedPrompts
    const item = queue[index]
    if (item === undefined) return
    set({queuedPrompts: queue.filter((_, i) => i !== index)})
    void get().sendPrompt(item.text, item.mode)
  },

  sendPrompt: async (msg, mode: 'normal' | 'follow_up' = 'normal') => {
    const session = get().session
    if (!session?.current_project_id) {
      useStore.getState().announce('Open or create a project before sending')
      set({error: 'Open or create a project before sending.'})
      return
    }
    // Queue while a run is in flight: the prompt stays visible/removable in
    // the strip and auto-sends when the current turn ends.
    const lastAgentNow = [...get().chat].reverse().find((m) => m.role === 'agent')
    if (lastAgentNow?.status === 'thinking' || lastAgentNow?.status === 'tool-calling') {
      set((s) => ({queuedPrompts: [...s.queuedPrompts, {text: msg, mode}]}))
      return
    }
    // Staged attachments are referenced by project-relative path so the
    // harness (spawning in the project dir) can read them; then cleared.
    const atts = get().attachments
    const finalMsg =
      atts.length > 0
        ? `${msg}\n\nAttachments:\n${atts
            .map((a) => `- ${a.info.rel_path} (${a.info.name})`)
            .join('\n')}`
        : msg
    set((s) => {
      const chat = [...s.chat]
      const idx = chat.findIndex((m) => m.role === 'agent' && m.status === 'thinking')
      if (idx >= 0) chat[idx] = {...chat[idx], status: 'thinking', startedAtMs: Date.now()}
      chat.push({id: `u${Date.now()}`, role: 'you', content: finalMsg, tools: [], mode})
      chat.push({
        id: `a${Date.now()}`,
        role: 'agent',
        content: '',
        status: 'thinking',
        startedAtMs: Date.now(),
        tools: [],
      })
      return {chat}
    })
    if (atts.length > 0) get().clearAttachments()
    try {
      await Commands.sendPrompt(finalMsg, mode)
    } catch (e) {
      set((s) => ({
        chat: s.chat.map((m) =>
          m.role === 'agent' && m.status === 'thinking'
            ? {...m, status: 'error', content: String(e), failedPrompt: finalMsg}
            : m,
        ),
        error: String(e),
      }))
    }
  },

  // --- Composer attachments ------------------------------------------------
  // The webview has no fs access, so picked files are read as base64 here and
  // the Rust core copies them into <project>/assets (save_attachment).
  attachFiles: async (files) => {
    const list = Array.from(files)
    if (list.length === 0) return
    const session = get().session
    if (!session?.current_project_id) {
      useStore.getState().announce('Open a project before attaching files')
      set({error: 'Open a project before attaching files.'})
      return
    }
    for (const file of list) {
      // Backend enforces a 64MB ceiling — check the real byte size BEFORE
      // reading so an oversized file is rejected cheaply.
      if (file.size > 64 * 1024 * 1024) {
        useStore.getState().announce(`${file.name} exceeds the 64MB attachment limit`)
        continue
      }
      try {
        const isImage = file.type.startsWith('image/')
        const previewUrl = isImage ? URL.createObjectURL(file) : null
        const dataB64 = await new Promise<string>((resolve, reject) => {
          const reader = new FileReader()
          reader.onload = () => {
            const url = String(reader.result ?? '')
            resolve(url.split(',').pop() ?? '')
          }
          reader.onerror = () => reject(new Error(`could not read ${file.name}`))
          reader.readAsDataURL(file)
        })
        const info = await Commands.saveAttachment(file.name, dataB64, isImage ? 'image' : 'file')
        set((s) => ({
          attachments: [...s.attachments, {info, previewUrl}],
        }))
      } catch (e) {
        useStore.getState().announce(`Could not attach ${file.name}`)
        set((s) => ({error: String(e)}))
      }
    }
  },

  removeAttachment: (id) =>
    set((s) => {
      const att = s.attachments.find((a) => a.info.id === id)
      if (att?.previewUrl) URL.revokeObjectURL(att.previewUrl)
      return {attachments: s.attachments.filter((a) => a.info.id !== id)}
    }),

  clearAttachments: () => {
    for (const att of get().attachments) {
      if (att.previewUrl) URL.revokeObjectURL(att.previewUrl)
    }
    set({attachments: []})
  },

  steer: async (msg) => {
    const runActive = [...get().chat].some(
      (m) => m.role === 'agent' && (m.status === 'thinking' || m.status === 'tool-calling'),
    )
    if (!runActive) {
      // Nothing is running — a steer has no turn to steer, so send normally.
      return get().sendPrompt(msg, 'normal')
    }
    set((s) => {
      const chat = [...s.chat]
      const idx = chat.findIndex(
        (m) => m.role === 'agent' && m.status !== 'done' && m.status !== 'error',
      )
      if (idx >= 0) chat[idx] = {...chat[idx], status: 'tool-calling'}
      chat.push({id: `u${Date.now()}`, role: 'you', content: msg, tools: [], mode: 'steer'})
      return {chat}
    })
    try {
      await Commands.steer(msg)
      useStore.getState().announce('Steer sent')
    } catch (e) {
      useStore.getState().announce('Steer failed')
      set({error: String(e)})
    }
  },

  abort: async () => {
    try {
      await Commands.abort()
    } catch (e) {
      set({error: String(e)})
    }
  },

  render: async (quality = 'draft', target) => {
    const session = get().session
    if (!session?.current_project_id) {
      set({error: 'Open or create a project before rendering.'})
      return
    }
    try {
      await Commands.renderToVideo(
        session.current_project_id,
        session.current_composition_id ?? 'main',
        {quality, target},
      )
      await get().refreshRenders()
      useStore.getState().announce('Render started')
    } catch (e) {
      useStore.getState().announce('Render failed')
      set({error: String(e)})
    }
  },

  cancelRender: async (jobId) => {
    try {
      await Commands.cancelRender(jobId)
      await get().refreshRenders()
      useStore.getState().announce('Render cancelled')
    } catch (e) {
      useStore.getState().announce('Could not cancel render')
      set({error: String(e)})
    }
  },

  approve: async (answer, alwaysAllow, value) => {
    const req = get().pendingApproval
    if (!req) return
    // allow → approve; edit → approve carrying the user's edited input
    // (a-coder-cli input/editor dialogs); deny → cancel.
    const approved = answer === 'allow' || answer === 'edit'
    set({pendingApproval: null})
    try {
      await Commands.approve(
        req.id,
        req.kind,
        req.payload,
        approved,
        alwaysAllow,
        answer === 'edit' ? (value ?? undefined) : undefined,
      )
    } catch (e) {
      set({error: String(e)})
    }
  },

  // --- Preview (Timeline tab) --------------------------------------------
  // Start the live preview server for the current composition. Idempotent:
  // a running server is left alone; a failed one is retried.
  startPreview: async () => {
    const {preview} = get()
    if (preview.status === 'running' || preview.status === 'starting') return
    set({preview: {...preview, status: 'starting', error: null}})
    try {
      await Commands.startPreview()
    } catch (e) {
      set({preview: {status: 'error', url: null, port: null, error: String(e)}})
    }
  },

  // Stop the preview server (called when leaving the Timeline tab).
  stopPreview: async () => {
    try {
      await Commands.stopPreview()
    } catch {
      /* non-fatal — the Stopped event still clears state */
    }
  },

  // Parse the current composition into a timeline and select its first clip.
  loadTimeline: async (projectId, compositionId) => {
    try {
      const timeline = await Commands.getTimeline(projectId, compositionId)
      const selectedClipId = timeline.clips[0]?.id ?? null
      set({timeline, selectedClipId})
    } catch (e) {
      set({timeline: null, selectedClipId: null, error: String(e)})
    }
  },

  selectClip: (clipId) => set({selectedClipId: clipId}),

  // Dismiss the most recent typed studio error (Phase 15).
  dismissError: () => set({studioError: null, error: null}),
}))

/**
 * Coalesce high-frequency events (open-design lesson B1): buffer incoming
 * events and flush every ~50ms (design.md's token-streaming budget), merging
 * runs of adjacent TextDelta events into one so the store re-renders at most
 * ~20x/s per stream instead of per token. Buffer order is FIFO — merging only
 * collapses adjacent deltas, so all other event semantics are preserved.
 */
const EVENT_BATCH_MS = 50
function createEventCoalescer(sink: (ev: StudioEvent) => void): (ev: StudioEvent) => void {
  let buffer: StudioEvent[] = []
  let timer: ReturnType<typeof setTimeout> | null = null
  const flush = () => {
    timer = null
    const batch = buffer
    buffer = []
    let i = 0
    while (i < batch.length) {
      const ev = batch[i]
      if (ev.type === 'Harness' && Object.keys(ev.payload)[0] === 'TextDelta') {
        // Merge the run of adjacent TextDelta events into one.
        let text = String((ev.payload as Record<string, unknown>).TextDelta ?? '')
        i++
        while (
          i < batch.length &&
          ev.type === 'Harness' &&
          batch[i].type === 'Harness' &&
          Object.keys(batch[i].payload)[0] === 'TextDelta'
        ) {
          text += String((batch[i].payload as Record<string, unknown>).TextDelta ?? '')
          i++
        }
        sink({type: 'Harness', payload: {TextDelta: text}} as StudioEvent)
        continue
      }
      sink(ev)
      i++
    }
  }
  return (ev: StudioEvent) => {
    buffer.push(ev)
    if (timer === null) {
      timer = setTimeout(flush, EVENT_BATCH_MS)
    }
  }
}

/** Reduce a StudioEvent into store mutations. */
function handleEvent(
  ev: StudioEvent,
  set: (fn: (s: AppState) => Partial<AppState>) => void,
  _get: () => AppState,
) {
  if (ev.type === 'Harness') {
    handleHarnessEvent(ev.payload, set)
  } else if (ev.type === 'Render') {
    handleRenderEvent(ev.payload, set)
  } else if (ev.type === 'Project') {
    const payload = ev.payload as {
      AssetAdded?: {project_id: string; asset_id: string; path: string}
      Created?: {project_id: string; name: string}
      Switched?: {project_id: string; name: string}
      Updated?: {project_id: string; changed_at: number}
    }
    if ('AssetAdded' in payload && payload.AssetAdded) {
      handleAssetAdded(payload.AssetAdded, set)
    } else {
      // Created / Switched / Updated via the MCP tools — refresh the whole
      // project context (session, project list, renders) in one shot.
      void useStore.getState().refreshProjectContext()
    }
  } else if (ev.type === 'Sidecar') {
    handleSidecarEvent(ev.payload, set)
  } else if (ev.type === 'Preview') {
    handlePreviewEvent(ev.payload, set)
  } else if (ev.type === 'Error') {
    // Most recent typed studio error (Phase 15). Shows in the status strip
    // with a concrete user-action button; never a silent hang.
    set(() => ({studioError: ev.payload as StudioError}))
  }
}

/** Accumulate an asset pinned via the snapshot command (AssetAdded event). */
/** Infer an asset's display kind from its file extension — the AssetAdded
 *  event carries only the path, and the composer can attach non-image files. */
function inferAssetKind(path: string): string {
  const ext = path.toLowerCase().split('.').pop() ?? ''
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'avif'].includes(ext)) return 'image'
  if (['mp4', 'mov', 'webm', 'avi', 'mkv'].includes(ext)) return 'video'
  if (['mp3', 'wav', 'ogg', 'flac', 'm4a', 'aac'].includes(ext)) return 'audio'
  return 'file'
}

function handleAssetAdded(
  a: {project_id: string; asset_id: string; path: string},
  set: (fn: (s: AppState) => Partial<AppState>) => void,
) {
  set((s) => {
    if (s.assets.some((x) => x.id === a.asset_id)) return {}
    const asset: Asset = {
      id: a.asset_id,
      project_id: a.project_id,
      composition_id: null,
      path: a.path,
      kind: inferAssetKind(a.path),
      source: 'local',
      prompt: null,
      created_at_ms: Date.now(),
    }
    return {assets: [...s.assets, asset]}
  })
}

function handlePreviewEvent(
  ev: PreviewEvent,
  set: (fn: (s: AppState) => Partial<AppState>) => void,
) {
  const kind = Object.keys(ev)[0] as keyof PreviewEvent
  const p = (ev as Record<string, unknown>)[kind] as {url?: string; port?: number; error?: string}
  switch (kind) {
    case 'Started':
      set(() => ({
        preview: {status: 'running', url: p.url ?? null, port: p.port ?? null, error: null},
      }))
      break
    case 'Stopped':
      set(() => ({preview: {status: 'idle', url: null, port: null, error: null}}))
      break
    case 'Failed':
      set(() => ({
        preview: {
          status: 'error',
          url: null,
          port: null,
          error: p.error ?? 'preview failed to start',
        },
      }))
      break
  }
}

function handleSidecarEvent(
  ev: Record<string, unknown>,
  set: (fn: (s: AppState) => Partial<AppState>) => void,
) {
  const kind = Object.keys(ev)[0]
  const p = (ev as Record<string, unknown>)[kind] as Record<string, unknown>
  const name = String(p.name ?? '')
  set((s) => {
    // Every sidecar event seeds a line in the log drawer so lifecycle
    // transitions stay visible even when the process itself is quiet.
    const seed: SidecarLogLine | null =
      kind === 'LogLine'
        ? {ts: Date.now(), level: String(p.level ?? 'info'), message: String(p.message ?? '')}
        : kind === 'Starting'
          ? {ts: Date.now(), level: 'info', message: 'starting…'}
          : kind === 'Ready'
            ? {ts: Date.now(), level: 'ok', message: 'ready'}
            : kind === 'Exit'
              ? {ts: Date.now(), level: 'error', message: `exited with code ${p.code ?? '?'}`}
              : null
    const prevLogs = s.sidecarLogs?.[name] ?? []
    const sidecarLogs =
      seed !== null
        ? {...s.sidecarLogs, [name]: [...prevLogs, seed].slice(-MAX_SIDECAR_LOG_LINES)}
        : s.sidecarLogs
    const sidecars = (s.sidecars ?? []).map((sc) => {
      if (sc.name !== name) return sc
      if (kind === 'Starting') return {...sc, status: 'starting'}
      if (kind === 'Ready') return {...sc, status: 'running'}
      if (kind === 'Exit') {
        const code = Number(p.code ?? -1)
        return {...sc, status: 'exited', detail: `exit code ${code}`}
      }
      return sc
    })
    return {sidecars, sidecarLogs}
  })
}

function handleHarnessEvent(
  ev: HarnessEvent,
  set: (fn: (s: AppState) => Partial<AppState>) => void,
) {
  const kind = Object.keys(ev)[0] as keyof HarnessEvent
  const payload = (ev as Record<string, unknown>)[kind]

  set((s) => {
    const chat = [...s.chat]
    const lastAgent = [...chat].reverse().find((m) => m.role === 'agent')
    switch (kind) {
      case 'ApprovalRequest': {
        const p = payload as {id: string; kind: string; payload: unknown}
        // Surface the approval modal; keep chat untouched. The turn shows the
        // amber "waiting-user" dot while the dialog is open (derived in the UI
        // from pendingApproval, design.md §9).
        queueMicrotask(() => useStore.getState().announce(`Approval requested: ${p.kind}`))
        return {pendingApproval: {id: p.id, kind: p.kind, payload: p.payload}}
      }
      case 'TextDelta': {
        if (lastAgent) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id)
          if (idx >= 0) {
            chat[idx] = {
              ...chat[idx],
              content: chat[idx].content + String(payload),
              status: 'tool-calling',
            }
          }
        }
        break
      }
      case 'ToolStart': {
        // Structured tool card (design.md §3): the call renders as its own
        // expandable block instead of a line of text.
        const p = payload as {tool_id?: string; name: string; args?: unknown}
        if (lastAgent) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id)
          if (idx >= 0) {
            const m = chat[idx]
            const call: ToolCall = {
              id: p.tool_id ?? `t${Date.now()}-${m.tools.length}`,
              name: p.name,
              args: p.args ?? null,
              partial: '',
              result: null,
              isError: false,
              done: false,
              startedAtMs: Date.now(),
            }
            chat[idx] = {...m, status: 'tool-calling', tools: [...m.tools, call]}
          }
        }
        break
      }
      case 'ToolUpdate': {
        const p = payload as {tool_id: string; partial: string}
        if (lastAgent) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id)
          if (idx >= 0) {
            const m = chat[idx]
            chat[idx] = {
              ...m,
              tools: m.tools.map((t) =>
                t.id === p.tool_id && !t.done ? {...t, partial: t.partial + p.partial} : t,
              ),
            }
          }
        }
        break
      }
      case 'ToolEnd': {
        const p = payload as {tool_id: string; is_error: boolean; result: string | null}
        if (lastAgent) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id)
          if (idx >= 0) {
            const m = chat[idx]
            chat[idx] = {
              ...m,
              tools: m.tools.map((t) =>
                t.id === p.tool_id ? {...t, done: true, result: p.result, isError: p.is_error} : t,
              ),
            }
          }
        }
        break
      }
      case 'AgentEnd': {
        const p = payload as {success: boolean; message: string | null}
        if (lastAgent) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id)
          if (idx >= 0) {
            chat[idx] = {...chat[idx], status: p.success ? 'done' : 'error'}
            // Failure recovery (open-design): persist the prompt that failed
            // so the message card can offer a one-click retry.
            if (!p.success) {
              const lastYou = [...chat].reverse().find((m) => m.role === 'you')
              chat[idx] = {...chat[idx], failedPrompt: lastYou?.content}
            }
          }
          // Screen-reader announcement for the turn outcome.
          queueMicrotask(() =>
            useStore.getState().announce(p.success ? 'Agent turn complete' : 'Agent turn failed'),
          )
        }
        // Turn over — hand the next queued prompt (FIFO) to the store; the
        // actual send happens after this reducer returns (no side effects in
        // the updater), preserving its original mode (normal / follow-up).
        const queue = (s.queuedPrompts ?? []).slice()
        const next = queue.shift()
        if (next !== undefined) {
          queueMicrotask(() => void useStore.getState().sendPrompt(next.text, next.mode))
        }
        return {chat, queuedPrompts: next !== undefined ? queue : s.queuedPrompts}
      }
      case 'QueueUpdate': {
        // Backend-side queueing (steer/follow-up accepted into the running
        // turn). Announce so the user gets feedback even if the UI strip
        // already shows the pending item.
        const p = payload as {steer: boolean; follow_up: boolean}
        queueMicrotask(() =>
          useStore
            .getState()
            .announce(
              p.follow_up
                ? 'Follow-up queued after the current turn'
                : 'Steer queued into the running turn',
            ),
        )
        break
      }
      case 'Error': {
        if (lastAgent) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id)
          if (idx >= 0) chat[idx] = {...chat[idx], status: 'error', content: String(payload)}
        } else {
          chat.push({id: `e${Date.now()}`, role: 'system', content: String(payload), tools: []})
        }
        break
      }
      case 'AgentStart': {
        // mark the trailing agent message as thinking
        if (lastAgent && lastAgent.status === undefined) {
          const idx = chat.findIndex((m) => m.id === lastAgent.id)
          if (idx >= 0) chat[idx] = {...chat[idx], status: 'thinking'}
        }
        break
      }
      default:
        break
    }
    return {chat}
  })
}

function handleRenderEvent(ev: RenderEvent, set: (fn: (s: AppState) => Partial<AppState>) => void) {
  const kind = Object.keys(ev)[0]
  const p = (ev as Record<string, unknown>)[kind] as Record<string, unknown>
  const jobId = String(p.job_id)
  set((s) => {
    const renders = [...s.renders]
    const idx = renders.findIndex((r) => r.job_id === jobId)
    if (kind === 'Started') {
      if (idx < 0) {
        renders.push({
          job_id: jobId,
          project_id: '',
          composition_id: '',
          target: String(p.target),
          quality: String(p.quality),
          status: 'running',
          started_at_ms: Date.now(),
          finished_at_ms: null,
          output_path: null,
          error: null,
        })
      }
    } else if (kind === 'Completed') {
      if (idx >= 0) {
        renders[idx] = {
          ...renders[idx],
          status: 'done',
          output_path: String(p.output_path),
          finished_at_ms: Date.now(),
          progress: null,
        }
      }
      queueMicrotask(() => useStore.getState().announce('Render completed'))
    } else if (kind === 'Progress') {
      const progress = {
        stage: String(p.stage ?? ''),
        frame: Number(p.frame ?? 0),
        total_frames: p.total_frames == null ? null : Number(p.total_frames),
      }
      if (idx >= 0) {
        renders[idx] = {...renders[idx], progress}
      } else {
        renders.push({
          job_id: jobId,
          project_id: '',
          composition_id: '',
          target: '',
          quality: '',
          status: 'running',
          started_at_ms: Date.now(),
          finished_at_ms: null,
          output_path: null,
          error: null,
          progress,
        })
      }
    } else if (kind === 'Failed') {
      if (idx >= 0) {
        renders[idx] = {
          ...renders[idx],
          status: 'failed',
          error: String(p.error),
          finished_at_ms: Date.now(),
        }
      }
      queueMicrotask(() => useStore.getState().announce(`Render failed: ${String(p.error)}`))
    } else if (kind === 'Cancelled') {
      if (idx >= 0)
        renders[idx] = {...renders[idx], status: 'cancelled', finished_at_ms: Date.now()}
    }
    return {renders}
  })
}
