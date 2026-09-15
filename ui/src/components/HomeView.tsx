/**
 * HomeView — the post-onboarding landing surface.
 *
 * Composition (open-design HomeView + HomeHero + RecentProjectsStrip idiom):
 *   1. Centered hero block on a faint dot wash:
 *        wordmark, tagline, row of scenario pills (TypePillRow equivalent),
 *        composer card with active-attachment chip row above the textarea,
 *        send bar (Send / Steer / ⌘↵ hint).
 *   2. If the chat has messages, the most recent few float above the hero
 *      so users see the live conversation without leaving Home.
 *   3. Stacked `entry-section` blocks below: Recent Projects (horizontal
 *      strip), Active Renders (compact queue summary), Tools (MCP /
 *      render-worker pills).
 *
 * The composer here IS the chat input — selecting Home never strands the
 * user; the old Chat tab is subsumed by Home.
 */

import {useEffect, useState} from 'react'
import {useStore} from '../lib/store'
import {elapsedLabel, formatRelative, swatchFor} from '../lib/format'
import {Composer} from './Composer'
import {ThinkingBlock} from './ThinkingBlock'

interface ScenarioPill {
  id: string
  label: string
  glyph: string
  /** Seed text inserted into the composer when the pill is clicked. */
  seed: string
}

const SCENARIO_PILLS: ScenarioPill[] = [
  {id: 'render', label: 'Render', glyph: '▷', seed: 'Render the current composition to MP4.'},
  {id: 'storyboard', label: 'Storyboard', glyph: '▤', seed: 'Write a 6-shot storyboard for: '},
  {id: 'edit', label: 'Edit', glyph: '✎', seed: 'Edit the current composition: '},
  {id: 'brand', label: 'Brand', glyph: '✸', seed: 'Apply brand tokens and re-style: '},
  {
    id: 'analyze',
    label: 'Analyze',
    glyph: '◌',
    seed: 'Analyze the current composition and suggest improvements: ',
  },
]

/** Copy-to-clipboard affordance on settled agent messages (open-design:
    copy-markdown on every settled message). Flash confirms the copy. */
function CopyButton({text}: {text: string}) {
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return
    const t = setTimeout(() => setCopied(false), 1200)
    return () => clearTimeout(t)
  }, [copied])
  return (
    <button
      type="button"
      className="icon-btn"
      title={copied ? 'Copied' : 'Copy message'}
      aria-label="Copy message"
      onClick={() => {
        navigator.clipboard?.writeText(text).then(
          () => setCopied(true),
          () => setCopied(false),
        )
      }}
    >
      {copied ? '✓' : '⧉'}
    </button>
  )
}

export function HomeView({
  onNavigate,
}: {
  onNavigate: (
    id: 'chat' | 'projects' | 'models' | 'sources' | 'tools' | 'renders' | 'settings',
  ) => void
}) {
  const session = useStore((s) => s.session)
  const projects = useStore((s) => s.projects) ?? []
  const renders = useStore((s) => s.renders) ?? []
  const harnesses = useStore((s) => s.harnesses) ?? []
  const sidecars = useStore((s) => s.sidecars) ?? []
  const chat = useStore((s) => s.chat)
  const sendPrompt = useStore((s) => s.sendPrompt)
  const setDraft = useStore((s) => s.setDraft)
  const openProject = useStore((s) => s.openProject)
  const refreshRenders = useStore((s) => s.refreshRenders)

  const current = projects.find((p) => p.id === session?.current_project_id)

  const [activePill, setActivePill] = useState<string | null>(null)
  // 1s ticker for the in-flight run's elapsed clock (open-design: anchored to
  // the persisted run start, not mount time). Only ticks while a run is live.
  const [nowMs, setNowMs] = useState(() => Date.now())
  const runActive = (chat ?? []).some(
    (m) => m.role === 'agent' && (m.status === 'thinking' || m.status === 'tool-calling'),
  )
  useEffect(() => {
    if (!runActive) return
    setNowMs(Date.now())
    const t = setInterval(() => setNowMs(Date.now()), 1000)
    return () => clearInterval(t)
  }, [runActive])

  // Keep renders fresh on Home view; the StatusStrip already calls this on
  // mount, but we want the active-renders section to update promptly too.
  useEffect(() => {
    void refreshRenders()
  }, [refreshRenders])

  const pickPill = (pill: ScenarioPill) => {
    setActivePill(pill.id)
    // Seed the shared composer draft (the Composer textarea owns focus).
    setDraft(pill.seed)
    requestAnimationFrame(() => {
      const ta = document.querySelector<HTMLTextAreaElement>('.composer__textarea')
      ta?.focus()
    })
  }

  // Show at most the last 4 messages above the composer — the full history
  // lives in the dedicated Chat surface.
  const recentMessages = (chat ?? []).slice(-4)

  const activeRenders = renders
    .filter((r) => r.status === 'running' || r.status === 'queued')
    .slice(0, 3)

  // Tool pills are sourced from real sidecar health — when a worker is up,
  // it shows as an enabled pill; offline workers show muted with their last
  // status. Cheap and truthful.
  const toolsPills = [
    {
      id: 'render-worker',
      label: 'render-worker',
      detail: sidecars.find((s) => s.name.includes('render'))?.status ?? 'idle',
    },
    {id: 'mcp', label: 'mcp tools', detail: 'stdio'},
    {id: 'skill', label: 'skills', detail: 'loaded'},
  ]

  return (
    <div className="home-wash entry-main__scroll-inner">
      <div className="home-hero">
        <div className="home-hero__kicker reveal">Amaara Studio</div>
        <h1 className="home-hero__logo reveal reveal-1">
          Direct an agent.
          <br />
          <em>Watch it make.</em>
        </h1>
        <p className="home-hero__tagline reveal reveal-2">
          Type what you want. The agent storyboards, authors the composition, and renders it
          to video — while you watch every step.
        </p>

        <div className="home-pill-row reveal reveal-3" role="tablist" aria-label="Scenarios">
          {SCENARIO_PILLS.map((pill) => (
            <button
              key={pill.id}
              type="button"
              role="tab"
              aria-selected={activePill === pill.id}
              className={`home-pill ${activePill === pill.id ? 'is-active' : ''}`}
              onClick={() => pickPill(pill)}
            >
              <span className="home-pill__glyph" aria-hidden>
                {pill.glyph}
              </span>
              {pill.label}
            </button>
          ))}
        </div>

        {recentMessages.length > 0 && (
          <div className="home-hero__history-wrap reveal reveal-2">
            <div className="home-hero__history-link">
              <button
                type="button"
                className="entry-section__action"
                onClick={() => onNavigate('chat')}
              >
                Open full chat →
              </button>
            </div>
            <div className="home-hero__history" aria-label="Recent conversation">
              {recentMessages.map((m) => (
                <div
                  key={m.id}
                  className={`home-hero__history-msg home-hero__history-msg--${m.role} ${
                    m.status === 'thinking' ? 'home-hero__history-msg--thinking' : ''
                  } ${m.status === 'error' ? 'home-hero__history-msg--error' : ''}`}
                >
                  <div className="home-hero__history-msg-head">
                    <span>
                      {m.role === 'you'
                        ? 'You'
                        : m.role === 'agent'
                          ? `Agent${session?.harness ? ` · ${session.harness}` : ''}`
                          : 'System'}
                    </span>
                    {m.role === 'agent' && m.status === 'done' && m.content && (
                      <CopyButton text={m.content} />
                    )}
                    {/* open-design: preparing → working distinction, never an
                      opaque spinner. Elapsed clock anchored to run start. */}
                    {m.status === 'thinking' &&
                      (!m.content ? (
                        <span className="shimmer-text">· preparing…</span>
                      ) : (
                        <span>
                          · working {m.startedAtMs ? elapsedLabel(m.startedAtMs, nowMs) : '…'}
                        </span>
                      ))}
                    {m.status === 'tool-calling' && (
                      <span>
                        · working {m.startedAtMs ? elapsedLabel(m.startedAtMs, nowMs) : '…'}
                      </span>
                    )}
                    {m.status === 'done' && <span>· done</span>}
                    {m.status === 'error' && <span>· error</span>}
                  </div>
                  <div className="home-hero__history-msg-body">
                    {m.content || (m.status === 'thinking' ? '…' : '')}
                    {m.role === 'agent' &&
                      (m.status === 'thinking' || m.status === 'tool-calling') &&
                      m.content && <span className="stream-caret">▍</span>}
                  </div>
                  {m.role === 'agent' && m.thinking && (
                    <ThinkingBlock text={m.thinking} live={m.status === 'thinking' || m.status === 'tool-calling'} />
                  )}
                  {/* Failure recovery (open-design): one-click retry of the
                    prompt this failed turn was answering. */}
                  {m.role === 'agent' && m.status === 'error' && m.failedPrompt && (
                    <div className="home-hero__history-msg-recover">
                      <button
                        type="button"
                        className="btn btn-sm"
                        onClick={() => void sendPrompt(m.failedPrompt!, 'normal')}
                      >
                        ↻ Retry
                      </button>
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="home-hero__composer-card reveal reveal-4">
          <div className="home-hero__active-row">
            {current ? (
              <span
                className="home-active-chip"
                title={`harness ${current.harness} · model ${current.model}`}
              >
                <span aria-hidden style={{fontSize: 10, color: 'var(--brand)'}}>
                  ●
                </span>
                <span className="home-active-chip__label">{current.name}</span>
                <span className="home-active-chip__meta">{current.harness}</span>
              </span>
            ) : (
              <span className="home-active-chip home-active-chip--placeholder">
                <span className="home-active-chip__label">
                  No project open — pick or create one in Projects
                </span>
              </span>
            )}
            <span className="home-active-chip">
              <span aria-hidden style={{fontSize: 10, color: 'var(--text-faint)'}}>
                ◇
              </span>
              <span className="home-active-chip__label">{session?.model ?? 'model'}</span>
            </span>
          </div>
          {/* Shared composer (draft, attachments, queued sends live in the
              store — identical behaviour on Home and the Chat surface). */}
          <Composer
            disabled={!current}
            placeholder={
              current
                ? 'Describe what you want to make. The agent will author the composition and render it.'
                : 'Open a project to enable the agent.'
            }
          />
        </div>
      </div>

      {/* Recent Projects — horizontal strip */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Recent Projects</h2>
          <div className="entry-section__actions">
            <button className="entry-section__action" onClick={() => onNavigate('projects')}>
              View all →
            </button>
          </div>
        </div>
        <div className="recent-projects-strip" role="list">
          {projects.length === 0 && (
            <div className="recent-projects-strip__empty">
              No projects yet. Create one to start working.
            </div>
          )}
          {projects.slice(0, 8).map((p) => (
            <button
              key={p.id}
              type="button"
              role="listitem"
              className="recent-project-card"
              onClick={() => void openProject(p.id)}
            >
              <div
                className="recent-project-card__swatch"
                style={{background: swatchFor(p.id)}}
                aria-hidden
              >
                <span className="recent-project-card__initial">
                  {p.name.charAt(0).toUpperCase()}
                </span>
              </div>
              <div className="recent-project-card__name" title={p.name}>
                {p.name}
              </div>
              <div className="recent-project-card__meta">
                <span className="recent-project-card__dir" title={p.dir}>
                  {p.dir}
                </span>
                <span>{formatRelative(p.created_at_ms)}</span>
              </div>
            </button>
          ))}
          <button
            type="button"
            className="recent-projects-strip__new-card"
            onClick={() => onNavigate('projects')}
          >
            <span className="recent-projects-strip__new-card-glyph" aria-hidden>
              +
            </span>
            New project
          </button>
        </div>
      </section>

      {/* Active Renders — compact queue summary */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Active Renders</h2>
          <div className="entry-section__actions">
            <button className="entry-section__action" onClick={() => onNavigate('renders')}>
              View queue →
            </button>
          </div>
        </div>
        {activeRenders.length === 0 ? (
          <div className="list-card">
            <div
              className="list-card__body"
              style={{padding: '14px 18px', color: 'var(--text-faint)', fontSize: 12}}
            >
              No renders running. Hit <span style={{fontFamily: 'var(--mono)'}}>Render</span> from
              the top bar to start one.
            </div>
          </div>
        ) : (
          <div className="list-card">
            <div className="list-card__body">
              {activeRenders.map((r) => (
                <div key={r.job_id} className="list-card__row">
                  <span style={{color: 'var(--info)', fontSize: 11}} aria-hidden>
                    ▶
                  </span>
                  <span className="list-card__row-title">
                    <span style={{fontFamily: 'var(--mono)', fontSize: 11}}>
                      {r.job_id.slice(0, 8)}
                    </span>
                    <span style={{color: 'var(--text-muted)'}}> · {r.quality || 'draft'}</span>
                  </span>
                  <span className="list-card__row-meta">{r.target || 'mp4'}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </section>

      {/* Tools */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Tools</h2>
          <div className="entry-section__actions">
            <button className="entry-section__action" onClick={() => onNavigate('tools')}>
              Configure →
            </button>
          </div>
        </div>
        <div style={{display: 'flex', flexWrap: 'wrap', gap: 6}}>
          {toolsPills.map((t) => {
            const up = t.detail === 'running' || t.detail === 'loaded'
            return (
              <span
                key={t.id}
                className="chip"
                style={{
                  opacity: up ? 1 : 0.55,
                  cursor: 'default',
                }}
                title={`${t.label} · ${t.detail}`}
              >
                <span
                  aria-hidden
                  style={{
                    fontSize: 9,
                    color: up ? 'var(--brand)' : 'var(--text-faint)',
                  }}
                >
                  ●
                </span>
                {t.label}
                <span
                  style={{
                    fontFamily: 'var(--mono)',
                    fontSize: 10,
                    color: 'var(--text-soft)',
                    marginLeft: 4,
                  }}
                >
                  {t.detail}
                </span>
              </span>
            )
          })}
          <span className="chip" style={{cursor: 'default', opacity: 0.85}} title="Active harness">
            {harnesses.find((h) => h.id === session?.harness)?.label ??
              session?.harness ??
              'harness'}
          </span>
        </div>
      </section>
    </div>
  )
}
