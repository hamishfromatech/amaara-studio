/**
 * ProjectsView — a card grid for the projects surface.
 *
 * open-design's `DesignsTab` shows projects as visual cards; amaara's
 * rail-as-list approach was less inviting. This view uses the same
 * recent-project card vocabulary as Home (deterministic swatch, mono
 * timestamp), with a header that mirrors the entry-section idiom and an
 * inline new-project form that drops open without breaking the grid.
 */

import {useState} from 'react'
import {useStore} from '../lib/store'
import {formatRelative, swatchFor} from '../lib/format'
import {EmptyState} from './EmptyState'

export function ProjectsView() {
  const projects = useStore((s) => s.projects) ?? []
  const session = useStore((s) => s.session)
  const newProject = useStore((s) => s.newProject)
  const openProject = useStore((s) => s.openProject)
  const deleteProject = useStore((s) => s.deleteProject)

  const [creating, setCreating] = useState(false)
  const [name, setName] = useState('')
  const [dir, setDir] = useState('')
  /** Project id armed for delete (two-click confirm, no native dialog). */
  const [confirmId, setConfirmId] = useState<string | null>(null)

  return (
    <div className="entry-main__scroll-inner">
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Projects</h2>
          <div className="entry-section__actions">
            <span style={{color: 'var(--text-faint)', fontFamily: 'var(--mono)', fontSize: 10}}>
              {projects.length} total
            </span>
          </div>
        </div>

        {creating && (
          <form
            className="new-project-form"
            style={{marginBottom: 12}}
            onSubmit={async (e) => {
              e.preventDefault()
              if (!name.trim() || !dir.trim()) return
              try {
                await newProject(name.trim(), dir.trim())
                setName('')
                setDir('')
                setCreating(false)
              } catch {
                /* the store surfaces the failure in the status strip */
              }
            }}
          >
            <div style={{display: 'flex', gap: 8}}>
              <input
                className="input input-sm"
                placeholder="Project name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                autoFocus
              />
              <input
                className="input input-sm"
                placeholder="Directory (e.g. ~/projects/foo)"
                value={dir}
                onChange={(e) => setDir(e.target.value)}
                style={{flex: 1}}
              />
            </div>
            <div style={{display: 'flex', gap: 6}}>
              <button
                type="submit"
                className="btn btn-primary btn-sm"
                disabled={!name.trim() || !dir.trim()}
              >
                Create
              </button>
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={() => {
                  setCreating(false)
                  setName('')
                  setDir('')
                }}
              >
                Cancel
              </button>
            </div>
          </form>
        )}

        <div className="project-card-grid">
          {projects.length === 0 && !creating ? (
            <EmptyState
              glyph="◳"
              title="No projects yet"
              description="Create your first project to start composing and rendering."
              action={
                <button
                  type="button"
                  className="btn btn-primary btn-sm"
                  onClick={() => setCreating(true)}
                >
                  + New project
                </button>
              }
            />
          ) : (
            <>
              {projects.map((p) => {
                const isActive = session?.current_project_id === p.id
                const armed = confirmId === p.id
                return (
                  <div
                    key={p.id}
                    className={`project-card ${isActive ? 'is-active' : ''}`}
                    role="button"
                    tabIndex={0}
                    aria-current={isActive ? 'true' : undefined}
                    onClick={() => armed ? setConfirmId(null) : void openProject(p.id)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && !armed) void openProject(p.id)
                    }}
                  >
                    <div className="project-card__head">
                      <div
                        className="project-card__swatch"
                        style={{background: swatchFor(p.id)}}
                        aria-hidden
                      >
                        <span className="recent-project-card__initial">
                          {p.name.charAt(0).toUpperCase()}
                        </span>
                      </div>
                      <div style={{flex: 1, minWidth: 0}}>
                        <div className="project-card__title">{p.name}</div>
                        <div className="project-card__dir" title={p.dir}>
                          {p.dir}
                        </div>
                      </div>
                      <button
                        type="button"
                        className={`project-card__delete ${armed ? 'is-armed' : ''}`}
                        title={armed ? 'Click again to delete this project' : 'Delete project (files on disk are kept)'}
                        aria-label={`Delete project ${p.name}`}
                        onClick={(e) => {
                          e.stopPropagation()
                          if (armed) {
                            void deleteProject(p.id)
                            setConfirmId(null)
                          } else {
                            setConfirmId(p.id)
                          }
                        }}
                      >
                        {armed ? 'delete?' : '✕'}
                      </button>
                    </div>
                    <div className="project-card__meta">
                      <span className="project-card__chip">{p.harness}</span>
                      <span>{formatRelative(p.created_at_ms)}</span>
                    </div>
                  </div>
                )
              })}

              {!creating && (
                <button
                  type="button"
                  className="project-card-grid__new"
                  onClick={() => setCreating(true)}
                >
                  + New project
                </button>
              )}
            </>
          )}
        </div>
      </section>
    </div>
  )
}
