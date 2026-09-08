/**
 * Labeled nav rail — the post-onboarding navigation surface.
 *
 * Borrows the `EntryNavRail` vocabulary from open-design: a 220px column
 * with icons + labels grouped under section headings, a hairline divider
 * between groups, and a quiet footer. Each item shows an optional count
 * badge (e.g. render queue length, project count).
 *
 * The rail is purely navigation — content lives in the center pane. The
 * Shell owns which item is active and renders the matching view.
 */

import {useState, type ReactNode} from 'react'
import {useStore} from '../lib/store'
import type {NavId} from '../lib/nav'

export type {NavId} from '../lib/nav'

interface NavItem {
  id: NavId
  label: string
  glyph: string
  /** Optional count badge content (e.g. number, dot). */
  badge?: string
  /** Show a small brand-green dot to the right of the label. */
  dot?: boolean
  /** Optional inline action rendered to the right of the label (new project / render). */
  action?: ReactNode
}

interface NavGroup {
  label: string
  items: NavItem[]
}

interface Props {
  navId: NavId
  onChange: (id: NavId) => void
}

export function EntryNavRail({navId, onChange}: Props) {
  const projects = useStore((s) => s.projects) ?? []
  const renders = useStore((s) => s.renders) ?? []
  const session = useStore((s) => s.session)
  const hasApiKey = useStore((s) => s.has_api_key) ?? false
  const previewRunning = useStore((s) => s.preview.status === 'running')
  const newProject = useStore((s) => s.newProject)
  const render = useStore((s) => s.render)

  const runningRenders = renders.filter((r) => r.status === 'running').length

  // Inline "new project" form state (toggled by the + in the Workspace group).
  const [creating, setCreating] = useState(false)
  const [name, setName] = useState('')
  const [dir, setDir] = useState('')
  // Inline "new render" state (toggled by the ▷ on the Renders row).
  const [rendering, setRendering] = useState(false)
  const [renderQuality, setRenderQuality] = useState('draft')

  const chat = useStore((s) => s.chat) ?? []
  const runActive = chat.some(
    (m) => m.role === 'agent' && (m.status === 'thinking' || m.status === 'tool-calling'),
  )
  const currentProjectId = session?.current_project_id
  const hasProject = !!currentProjectId

  const create = async () => {
    if (!name.trim() || !dir.trim()) return
    try {
      await newProject(name.trim(), dir.trim())
      setName('')
      setDir('')
      setCreating(false)
    } catch {
      /* surfaces via the status strip */
    }
  }

  const startRender = async () => {
    if (!hasProject) return
    try {
      await render(renderQuality)
    } finally {
      setRendering(false)
    }
  }

  const groups: NavGroup[] = [
    {
      label: 'Workspace',
      items: [
        {id: 'home', label: 'Home', glyph: '◐'},
        {id: 'chat', label: 'Chat', glyph: '◑', dot: runActive},
        {
          id: 'projects',
          label: 'Projects',
          glyph: '▤',
          badge: projects.length ? String(projects.length) : undefined,
          action: hasProject ? (
            <NewProjectInline
              open={creating}
              name={name}
              dir={dir}
              setName={setName}
              setDir={setDir}
              create={create}
              close={() => {
                setCreating(false)
                setName('')
                setDir('')
              }}
            />
          ) : null,
        },
        {
          id: 'timeline',
          label: 'Timeline',
          glyph: '▰',
          dot: previewRunning,
        },
        {
          id: 'renders',
          label: 'Renders',
          glyph: '▷',
          badge: runningRenders
            ? String(runningRenders)
            : renders.length
              ? String(renders.length)
              : undefined,
          dot: runningRenders > 0,
          action: hasProject ? (
            <RenderInline
              rendering={rendering}
              setRendering={setRendering}
              disabled={rendering || !hasProject}
              onStart={startRender}
            />
          ) : null,
        },
      ],
    },
    {
      label: 'Library',
      items: [
        {id: 'models', label: 'Models', glyph: '◆'},
        {id: 'sources', label: 'Sources', glyph: '⇄'},
        {id: 'tools', label: 'Tools', glyph: '⌬'},
      ],
    },
    {
      label: 'App',
      items: [
        {
          id: 'settings',
          label: 'Settings',
          glyph: '⚙',
          dot: !hasApiKey,
        },
      ],
    },
  ]

  const activeProjectName = projects.find((p) => p.id === session?.current_project_id)?.name ?? null

  return (
    <nav className="entry-nav-rail" aria-label="Studio navigation">
      <div className="entry-nav-rail__brand">
        <span className="entry-nav-rail__brand-mark" aria-hidden>
          ⬢
        </span>
        <span className="entry-nav-rail__brand-name">Navya Studio</span>
      </div>

      <div className="entry-nav-rail__scroll">
        {groups.map((group, gi) => (
          <div key={group.label} className="entry-nav-rail__group">
            {gi > 0 && <div className="entry-nav-rail__divider" />}
            <div className="entry-nav-rail__group-label">{group.label}</div>
            {group.items.map((item) => {
              const isActive = navId === item.id
              return (
                <button
                  key={item.id}
                  type="button"
                  className={`entry-nav-rail__btn ${isActive ? 'is-active' : ''}`}
                  aria-current={isActive ? 'page' : undefined}
                  onClick={() => onChange(item.id)}
                >
                  <span className="entry-nav-rail__btn-icon" aria-hidden>
                    {item.glyph}
                  </span>
                  <span className="entry-nav-rail__btn-label">{item.label}</span>
                  {item.dot && <span className="entry-nav-rail__btn-dot" aria-hidden />}
                  {item.badge && <span className="entry-nav-rail__btn-count">{item.badge}</span>}
                  {item.action && <div className="entry-nav-rail__btn-action">{item.action}</div>}
                </button>
              )
            })}
          </div>
        ))}
      </div>

      <div className="entry-nav-rail__footer">
        {activeProjectName ? (
          <div className="entry-nav-rail__footer-row">
            <span>Project</span>
            <span style={{color: 'var(--text)', fontFamily: 'var(--mono)', fontSize: 10}}>
              {activeProjectName}
            </span>
          </div>
        ) : (
          <div className="entry-nav-rail__footer-row">
            <span>No project open</span>
          </div>
        )}
        <div className="entry-nav-rail__footer-row" style={{marginTop: 4}}>
          <span>{hasApiKey ? 'Connected' : 'API key missing'}</span>
          <span
            aria-hidden
            style={{
              display: 'inline-block',
              width: 6,
              height: 6,
              borderRadius: '50%',
              background: hasApiKey ? 'var(--brand)' : 'var(--amber)',
            }}
          />
        </div>
      </div>
    </nav>
  )
}

/** Inline "new project" form in the Projects row (name + dir, Create/Cancel).
 *  Props are passed from EntryNavRail's render scope (avoids capturing a
 *  hoisted-function closure, which would otherwise trip use-before-define). */
function NewProjectInline({
  open,
  name,
  dir,
  setName,
  setDir,
  create,
  close,
}: {
  open: boolean
  name: string
  dir: string
  setName: (v: string) => void
  setDir: (v: string) => void
  create: () => void
  close: () => void
}) {
  if (!open) {
    return (
      <button
        type="button"
        className="icon-btn h-4 w-4 rail-action"
        aria-label="New project"
        title="New project"
      >
        ＋
      </button>
    )
  }
  return (
    <div className="rail-action-form">
      <input
        className="input input-sm rail-action-input"
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="Name"
        autoFocus
        aria-label="New project name"
      />
      <input
        className="input input-sm rail-action-input"
        value={dir}
        onChange={(e) => setDir(e.target.value)}
        placeholder="Directory (~/projects)"
        aria-label="New project directory"
      />
      <div className="rail-action-buttons">
        <button
          type="button"
          className="btn btn-primary btn-sm rail-action-btn"
          disabled={!name.trim() || !dir.trim()}
          onClick={create}
        >
          Create
        </button>
        <button type="button" className="btn btn-ghost btn-sm rail-action-btn" onClick={close}>
          Cancel
        </button>
      </div>
    </div>
  )
}

/** Inline "new render" action on the Renders row (draft/high quality). */
function RenderInline({
  rendering,
  setRendering,
  disabled,
  onStart,
}: {
  rendering: boolean
  setRendering: (v: boolean) => void
  disabled: boolean
  onStart: () => void
}) {
  return (
    <button
      type="button"
      className="icon-btn h-4 w-4 rail-action"
      aria-label={rendering ? 'Rendering…' : 'New render'}
      title={rendering ? 'Rendering…' : 'Render the current composition'}
      disabled={disabled}
      onClick={() => {
        onStart()
        setRendering(true)
      }}
    >
      {rendering ? '…' : '▷'}
    </button>
  )
}
