/**
 * Amaara Studio shell — state-driven (production wiring).
 *
 * Post-onboarding layout (open-design EntryShell idiom):
 *   TopBar  ──  EntryNavRail  ──  CenterPane (Home/Projects/Models/Sources/Tools/Renders/Settings)  ──  RightRail
 *
 * Every panel reads from the Zustand store, which is fed by the Rust core
 * through #[tauri::command] + studio://event. Pickers call the real commands;
 * the home composer renders real streamed events; the status strip reflects
 * real sidecar health. No hardcoded mock content.
 */

import {useEffect, useRef, useState} from 'react'
import {useStore} from '../lib/store'
import {useKeyboardShortcuts} from '../lib/shortcuts'
import {harnessHint, Commands} from '../lib/invoke'
import {userActionLabel} from '../lib/events'
import {ApprovalDialog} from './ApprovalDialog'
import {Inspector} from './Inspector'
import {CommandPalette} from './CommandPalette'
import {ShortcutsHelp} from './ShortcutsHelp'
import {Logo} from './Logo'
import {RendersTab} from '../renders/RendersTab'
import {TimelineTab} from '../timeline/TimelineTab'
import {EntryNavRail, type NavId} from './EntryNavRail'
import {HomeView} from './HomeView'
import {ChatView} from './ChatView'
import {ProjectsView} from './ProjectsView'
import {ModelsView} from './ModelsView'
import {SourcesView} from './SourcesView'
import {ToolsView} from './ToolsView'
import {SettingsView} from './SettingsView'

// --- TopBar ---
function TopBar() {
  const session = useStore((s) => s.session)
  const config = useStore((s) => s.config)
  const projects = useStore((s) => s.projects) ?? []
  const models = useStore((s) => s.models) ?? []
  const harnesses = useStore((s) => s.harnesses) ?? []
  const chat = useStore((s) => s.chat)
  const setModel = useStore((s) => s.setModel)
  const setSource = useStore((s) => s.setSource)
  const setHarness = useStore((s) => s.setHarness)
  const openProject = useStore((s) => s.openProject)
  const render = useStore((s) => s.render)
  const abort = useStore((s) => s.abort)
  const setTheme = useStore((s) => s.setTheme)

  if (!session || !config) return null
  const current = projects.find((p) => p.id === session.current_project_id)

  // Honest button states: Render needs a project; Stop needs a live run.
  const runActive = chat.some(
    (m) => m.role === 'agent' && (m.status === 'thinking' || m.status === 'tool-calling'),
  )
  // Grouped model options (harness first — it's what the prompt runs through).
  const bySource = (src: string) => models.filter((m) => m.source === src)
  const groups: [string, typeof models][] = [
    [`Harness — ${session.harness}`, bySource('harness')],
    ['Cloud (image)', bySource('cloud')],
    ['Engine', bySource('engine')],
    ['Local', bySource('local')],
  ]

  return (
    <header className="topbar">
      <div className="topbar__left">
        <span className="topbar__mark" title="Amaara Studio">
          <Logo size={18} />
        </span>
        <select
          className="topbar__select topbar__select--project"
          value={session.current_project_id ?? ''}
          onChange={(e) => e.target.value && void openProject(e.target.value)}
          aria-label="Project"
        >
          <option value="" disabled>
            {current ? current.name : 'No project open'}
          </option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </div>

      <div className="topbar__actions">
        <button
          className="btn btn-primary btn-sm"
          disabled={!session.current_project_id}
          title={
            session.current_project_id
              ? 'Render the current composition (⌘R)'
              : 'Open a project first'
          }
          onClick={() => void render('high')}
        >
          Render
        </button>
        <button
          className="btn btn-ghost btn-sm"
          disabled={!runActive}
          title={runActive ? 'Abort the current agent run (⌘.)' : 'No agent run in progress'}
          onClick={() => void abort()}
        >
          Stop
        </button>

        <span className="topbar__sep" aria-hidden />

        <select
          className="topbar__select"
          value={session.harness}
          onChange={(e) => void setHarness(e.target.value)}
          title={(() => {
            const h = harnesses.find((x) => x.id === session.harness)
            return h ? harnessHint(h) : 'Harness'
          })()}
          aria-label="Harness"
        >
          {harnesses.map((h) => (
            <option key={h.id} value={h.id} disabled={!h.available} title={harnessHint(h)}>
              {h.label} {h.available ? '' : '(not installed)'}
            </option>
          ))}
        </select>

        <select
          className="topbar__select topbar__select--model"
          value={session.model}
          onChange={(e) => void setModel(e.target.value)}
          title="Model"
          aria-label="Model"
        >
          {groups.map(
            ([label, group]) =>
              group.length > 0 && (
                <optgroup key={label} label={label}>
                  {group.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name} {m.kind !== 'chat' ? `(${m.kind})` : ''}
                    </option>
                  ))}
                </optgroup>
              ),
          )}
        </select>

        <div className="seg" title="Model source">
          <button
            className={session.source === 'cloud' ? 'is-active' : ''}
            onClick={() => void setSource('cloud')}
          >
            Cloud
          </button>
          <button
            className={session.source === 'local' ? 'is-active' : ''}
            onClick={() => void setSource('local')}
          >
            Local
          </button>
        </div>

        <button
          className="icon-btn topbar__theme"
          title={config.theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
          onClick={() => void setTheme(config.theme === 'dark' ? 'light' : 'dark')}
        >
          {config.theme === 'dark' ? '☀' : '☾'}
        </button>
      </div>
    </header>
  )
}

// --- RightRail ---
function RightRail() {
  const renders = useStore((s) => s.renders)
  const cancelRender = useStore((s) => s.cancelRender)
  const session = useStore((s) => s.session)
  const config = useStore((s) => s.config)
  const projects = useStore((s) => s.projects) ?? []
  const timeline = useStore((s) => s.timeline)
  const selectedClipId = useStore((s) => s.selectedClipId)
  const sendPrompt = useStore((s) => s.sendPrompt)

  const activeCount = renders.filter((r) => r.status === 'running').length
  const doneCount = renders.filter((r) => r.status === 'done').length
  const current = projects.find((p) => p.id === session?.current_project_id)
  const assets = useStore((s) => s.assets)

  return (
    <aside className="w-72 shrink-0 overflow-y-auto border-l border-line-soft bg-panel p-3 text-ink">
      <div className="mb-5">
        <h3 className="rail-label">Render Queue</h3>
        <div className="mb-3 grid grid-cols-2 gap-2">
          <div
            style={{
              padding: '8px 10px',
              background: 'var(--bg)',
              border: '1px solid var(--border-soft)',
              borderRadius: 8,
            }}
          >
            <div
              style={{
                fontFamily: 'var(--mono)',
                fontSize: 16,
                color: 'var(--text-strong)',
                lineHeight: 1,
              }}
            >
              {activeCount}
            </div>
            <div style={{fontSize: 10, color: 'var(--text-faint)', marginTop: 2}}>running</div>
          </div>
          <div
            style={{
              padding: '8px 10px',
              background: 'var(--bg)',
              border: '1px solid var(--border-soft)',
              borderRadius: 8,
            }}
          >
            <div
              style={{
                fontFamily: 'var(--mono)',
                fontSize: 16,
                color: 'var(--text-strong)',
                lineHeight: 1,
              }}
            >
              {doneCount}
            </div>
            <div style={{fontSize: 10, color: 'var(--text-faint)', marginTop: 2}}>done</div>
          </div>
        </div>

        <div className="space-y-1.5">
          {renders.length === 0 && (
            <div className="px-2 py-1 text-xs text-ink-faint">No renders yet.</div>
          )}
          {renders.slice(0, 5).map((r) => (
            <div
              key={r.job_id}
              className={`flex items-center justify-between gap-2 rounded-lg border border-line-soft bg-canvas px-2.5 py-2 ${
                r.status === 'done' ? 'render-done-flash' : ''
              }`}
            >
              <span className="flex min-w-0 items-center gap-2 text-[13px]">
                <RenderDot status={r.status} />
                <span className="truncate">
                  <span className="mono text-xs">{r.composition_id || r.job_id.slice(0, 8)}</span>
                  <span className="text-ink-muted"> · {r.quality}</span>
                </span>
              </span>
              <div className="flex shrink-0 gap-1 text-xs">
                {r.status === 'running' && (
                  <button
                    className="icon-btn h-6 w-6"
                    title="Cancel render"
                    onClick={() => void cancelRender(r.job_id)}
                  >
                    ✕
                  </button>
                )}
                {r.status === 'done' && r.output_path && (
                  <button
                    className="icon-btn h-6 w-6"
                    title="Reveal file"
                    onClick={() => void Commands.revealInFolder(r.output_path!)}
                  >
                    ↗
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
      </div>

      <div>
        <h3 className="rail-label">Inspector</h3>
        <div className="space-y-1 rounded-lg border border-line-soft bg-canvas p-3">
          {selectedClipId && timeline ? (
            <Inspector
              clip={timeline.clips.find((c) => c.id === selectedClipId) ?? null}
              onEdit={(instruction) => void sendPrompt(instruction)}
            />
          ) : (
            <div className="text-[13px] font-medium text-ink-strong">
              {current ? current.name : 'No project'}
            </div>
          )}
          {current && (
            <>
              <div className="mono truncate text-xs text-ink-muted" title={current.dir}>
                {current.dir}
              </div>
              <div className="text-xs text-ink-muted">
                harness <span className="text-ink">{current.harness}</span>
              </div>
              <div className="text-xs text-ink-muted">
                model <span className="text-ink">{current.model}</span>
              </div>
            </>
          )}
          {session && (
            <div className="mt-1 border-t border-line-soft pt-2">
              <div className="mb-1 text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-faint">
                Session
              </div>
              <div className="text-xs text-ink-muted">
                source <span className="text-ink">{session.source}</span>
              </div>
              <div className="text-xs text-ink-muted">
                model <span className="text-ink">{session.model}</span>
              </div>
            </div>
          )}
          {config && (
            <div className="mt-1 border-t border-line-soft pt-2">
              <div className="mb-1 text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-faint">
                Config
              </div>
              <div className="text-xs text-ink-muted">
                base{' '}
                <span style={{fontFamily: 'var(--mono)'}} className="text-ink">
                  {config.amaara_base_url}
                </span>
              </div>
              <div className="text-xs text-ink-muted">
                byok <span className="text-ink">{config.byok ? 'yes' : 'no'}</span>
              </div>
            </div>
          )}
          {assets.length > 0 && (
            <div className="mt-1 border-t border-line-soft pt-2">
              <div className="mb-1 flex items-center justify-between">
                <div className="text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-faint">
                  Assets
                </div>
                <span className="text-[10px] text-ink-faint">{assets.length}</span>
              </div>
              <div className="space-y-1">
                {assets.map((a) => (
                  <div
                    key={a.id}
                    className="flex items-center gap-2 rounded border border-line-soft bg-canvas px-2 py-1 text-xs"
                  >
                    <span className="text-ink">{a.kind}</span>
                    <span className="mono truncate text-ink-faint" title={a.path}>
                      {a.path}
                    </span>
                    <button
                      type="button"
                      className="icon-btn shrink-0 text-ink-faint hover:text-ink"
                      title="Reveal in folder"
                      aria-label={`Reveal ${a.path} in folder`}
                      onClick={() => void Commands.revealInFolder(a.path)}
                    >
                      ↗
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </aside>
  )
}

function RenderDot({status}: {status: string}) {
  const cls =
    status === 'running'
      ? 'text-info'
      : status === 'done'
        ? 'text-ok'
        : status === 'failed'
          ? 'text-danger'
          : 'text-ink-faint'
  const glyph =
    status === 'running' ? '▶' : status === 'done' ? '✓' : status === 'failed' ? '✗' : '○'
  return <span className={`text-xs ${cls}`}>{glyph}</span>
}

// --- StatusStrip ---
function StatusStrip() {
  const sidecars = useStore((s) => s.sidecars) ?? []
  const sidecarLogs = useStore((s) => s.sidecarLogs)
  const error = useStore((s) => s.error)
  const studioError = useStore((s) => s.studioError)
  const dismissError = useStore((s) => s.dismissError)
  const pendingApproval = useStore((s) => s.pendingApproval)
  // Live render count from the renders array (updated by events), not the
  // one-shot snapshot value which is stale after the initial load.
  const renderCount = useStore((s) => s.renders?.length ?? 0)
  // Which sidecar's log drawer is open (Phase 15); null = closed.
  const [openLog, setOpenLog] = useState<string | null>(null)

  const dot = (status: string) =>
    status === 'running' ? 'text-info' : status === 'exited' ? 'text-danger' : 'text-ink-faint'

  const activeLogs = openLog ? (sidecarLogs?.[openLog] ?? []) : []
  return (
    <div className="relative shrink-0">
      {openLog !== null && (
        <SidecarLogDrawer name={openLog} logs={activeLogs} onClose={() => setOpenLog(null)} />
      )}
      <footer className="flex h-7 shrink-0 items-center gap-4 border-t border-line-soft bg-panel px-3 text-xs text-ink-muted">
        {sidecars.map((sc) => (
          <button
            key={sc.name}
            type="button"
            className={`flex items-center gap-1.5 rounded px-1 py-0.5 transition-colors hover:bg-fill-secondary ${
              openLog === sc.name ? 'bg-fill-secondary text-ink' : ''
            }`}
            title={openLog === sc.name ? 'Close log' : 'View log'}
            onClick={() => setOpenLog(openLog === sc.name ? null : sc.name)}
          >
            <span className={`text-[9px] ${dot(sc.status)}`}>●</span>
            <span>
              {sc.name} <span className="text-ink-faint">{sc.status}</span>
            </span>
          </button>
        ))}
        <span className="numeric ml-auto text-ink-muted">{renderCount} render(s)</span>
        {pendingApproval && (
          <span
            className="flex items-center gap-1.5 rounded border border-warn-bg bg-warn-bg px-2 py-0.5 text-warn"
            title="The agent is waiting for your approval"
          >
            <span aria-hidden>⏸</span>
            <span>waiting-user · {pendingApproval.kind}</span>
          </span>
        )}
        {studioError && (
          <span
            className="flex items-center gap-2 rounded border border-danger-border bg-danger-bg px-2 py-0.5 text-danger"
            title={studioError.code}
          >
            <span className="max-w-xs truncate">⚠ {studioError.message}</span>
            <span className="text-ink-faint">— {userActionLabel(studioError.user_action)}</span>
            <button
              className="icon-btn h-4 w-4 shrink-0 text-danger"
              title="Dismiss"
              onClick={() => dismissError()}
            >
              ×
            </button>
          </span>
        )}
        {error && (
          <span className="flex items-center gap-1.5 max-w-xs truncate text-danger">
            <span className="max-w-xs truncate" title={error}>⚠ {error}</span>
            <button
              type="button"
              className="icon-btn shrink-0"
              title="Dismiss"
              aria-label="Dismiss error"
              onClick={() => dismissError()}
            >
              ×
            </button>
          </span>
        )}
      </footer>
    </div>
  )
}

/**
 * Per-sidecar log drawer (Phase 15): opens above the status strip when a
 * sidecar chip is clicked. Shows the tail of the sidecar's output, newest
 * last, auto-scrolled to the bottom.
 */
function SidecarLogDrawer({
  name,
  logs,
  onClose,
}: {
  name: string
  logs: {ts: number; level: string; message: string}[]
  onClose: () => void
}) {
  const scrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [logs.length])

  const levelTone = (level: string) =>
    level === 'error' ? 'text-danger' : level === 'ok' ? 'text-ok' : 'text-ink-muted'

  return (
    <div className="border-t border-line-soft bg-panel">
      <div className="flex items-center justify-between px-3 pb-1 pt-1.5">
        <span className="rail-label text-ink-muted">
          {name} — log (latest {logs.length})
        </span>
        <button className="icon-btn h-4 w-4 text-ink-muted" title="Close" onClick={onClose}>
          ×
        </button>
      </div>
      <div
        ref={scrollRef}
        className="mono max-h-44 overflow-y-auto px-3 pb-2 text-[11px] leading-relaxed"
      >
        {logs.length === 0 ? (
          <div className="py-3 text-ink-faint">no output yet — lines appear as {name} runs</div>
        ) : (
          logs.map((l, i) => (
            <div key={i} className="flex gap-2">
              <span className="numeric shrink-0 text-ink-faint">
                {new Date(l.ts).toLocaleTimeString()}
              </span>
              <span className={`whitespace-pre-wrap break-all ${levelTone(l.level)}`}>
                {l.message}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  )
}

// --- StudioShell (top-level) ---
export function StudioShell() {
  const navId = useStore((s) => s.navId)
  const setNavId = useStore((s) => s.setNavId)
  const railsHidden = useStore((s) => s.railsHidden)
  const refreshRenders = useStore((s) => s.refreshRenders)
  const pendingApproval = useStore((s) => s.pendingApproval)
  const approve = useStore((s) => s.approve)

  // Global keyboard shortcuts (Phase 16): ⌘K palette, tab/rail toggles,
  // timeline shuttle, etc. Attaches one window keydown listener.
  useKeyboardShortcuts()

  useEffect(() => {
    void refreshRenders()
  }, [refreshRenders])

  const navigate = (id: NavId) => setNavId(id)

  return (
    <div className="flex h-screen flex-col bg-canvas text-ink">
      <TopBar />
      <div className="flex flex-1 overflow-hidden">
        {!railsHidden.left && <EntryNavRail navId={navId} onChange={setNavId} />}
        <main className="entry-main">
          {navId === 'chat' ? (
            <ChatView />
          ) : (
            <div className="entry-main__scroll">
              {navId === 'home' && <HomeView onNavigate={navigate} />}
              {navId === 'projects' && <ProjectsView />}
              {navId === 'timeline' && <TimelineTab />}
              {navId === 'models' && <ModelsView />}
              {navId === 'sources' && <SourcesView />}
              {navId === 'tools' && <ToolsView />}
              {navId === 'renders' && <RendersTab />}
              {navId === 'settings' && <SettingsView />}
            </div>
          )}
        </main>
        {!railsHidden.right && <RightRail />}
      </div>
      <StatusStrip />
      {pendingApproval && (
        <ApprovalDialog
          request={pendingApproval}
          onAnswer={(answer, alwaysAllow, value) => {
            void approve(answer, alwaysAllow, value)
          }}
        />
      )}
      <RenderDialog />
      <CommandPalette />
      <ShortcutsHelp />
      {/* Screen-reader announcements (Phase 17 a11y): turn outcomes, render
          completions, approval requests. Polite — never interrupts speech. */}
      <LiveRegion />
    </div>
  )
}

/**
 * Visually-hidden aria-live region fed by store.announce(). A single region
 * with aria-live="polite"; the `n` counter re-triggers identical text.
 */
function LiveRegion() {
  const announcement = useStore((s) => s.announcement)
  return (
    <div aria-live="polite" aria-atomic="true" role="status" className="sr-only">
      {announcement.text}
      {/* key change forces a fresh node so repeated text re-announces */}
      <span key={announcement.n} hidden />
    </div>
  )
}

/**
 * Render… dialog (⌘⇧R, design.md §10): choose quality + target before
 * starting a render. The plain ⌘R / TopBar path uses the last defaults.
 */
function RenderDialog() {
  const open = useStore((s) => s.renderDialogOpen)
  const setOpen = useStore((s) => s.setRenderDialogOpen)
  const render = useStore((s) => s.render)
  const session = useStore((s) => s.session)
  const [quality, setQuality] = useState<'draft' | 'high'>('draft')
  const [target, setTarget] = useState('local')
  const dialogRef = useRef<HTMLDivElement | null>(null)
  const restoreRef = useRef<HTMLElement | null>(null)

  useEffect(() => {
    if (!open) return
    restoreRef.current = document.activeElement as HTMLElement | null
    const t = window.setTimeout(
      () => dialogRef.current?.querySelector<HTMLButtonElement>('[data-default]')?.focus(),
      0,
    )
    return () => {
      window.clearTimeout(t)
      restoreRef.current?.focus?.()
    }
  }, [open])

  if (!open) return null

  const start = () => {
    setOpen(false)
    void render(quality, target)
  }

  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && setOpen(false)}>
      <div
        ref={dialogRef}
        className="modal w-full max-w-sm p-5"
        role="dialog"
        aria-modal="true"
        aria-labelledby="render-dialog-title"
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault()
            e.stopPropagation()
            setOpen(false)
          }
        }}
      >
        <h3 id="render-dialog-title" className="mb-3 text-sm font-semibold text-ink-strong">
          Render…
        </h3>
        <div className="mb-4 space-y-3">
          <div>
            <div className="mb-1 text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-faint">
              Quality
            </div>
            <div className="seg" role="radiogroup" aria-label="Render quality">
              {(['draft', 'high'] as const).map((q) => (
                <button
                  key={q}
                  type="button"
                  role="radio"
                  aria-checked={quality === q}
                  data-default={q === 'draft' ? 'true' : undefined}
                  className={quality === q ? 'is-active' : ''}
                  onClick={() => setQuality(q)}
                >
                  {q}
                </button>
              ))}
            </div>
          </div>
          <div>
            <div className="mb-1 text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-faint">
              Target
            </div>
            <select
              className="input input-sm"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              aria-label="Render target"
            >
              <option value="local">local (npx hyperframes)</option>
              <option value="cloud">cloud</option>
              <option value="docker">docker</option>
              <option value="lambda">lambda</option>
              <option value="cloudrun">cloudrun</option>
            </select>
          </div>
        </div>
        <div className="flex justify-end gap-2">
          <button className="btn btn-ghost btn-sm" onClick={() => setOpen(false)}>
            Cancel
          </button>
          <button
            className="btn btn-primary btn-sm"
            disabled={!session?.current_project_id}
            onClick={start}
          >
            Render
          </button>
        </div>
      </div>
    </div>
  )
}
