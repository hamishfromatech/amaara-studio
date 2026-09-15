/**
 * SettingsView — all studio settings, persisted through save_config.
 *
 *   Amaara Cloud  — endpoint, default model, auto-router + BYOK flags,
 *                  and the API key (OS keychain, never echoed).
 *   Local models — llama.cpp server URL, Amaara Engine URL, sd-server
 *                  URL/binary/models dir, GPU backend (design.md §7).
 *   Harness      — which CLI the studio spawns.
 *   Appearance   — theme + density.
 *   Privacy      — opt-in analytics.
 *   About        — version, feedback packager.
 *
 * Text/flag fields edit a local draft; a Save button appears when the draft
 * diverges from the persisted config (one round-trip, honest dirty state).
 */

import {useEffect, useState} from 'react'
import {useStore} from '../lib/store'
import {harnessHint, Commands, type AmaaraConfig} from '../lib/invoke'

interface Draft {
  amaara_base_url: string
  default_model: string
  use_auto_router: boolean
  byok: boolean
  local_llama_url: string
  engine_url: string
  sd_server_url: string
  sd_binary_path: string
  sd_models_dir: string
  sd_gpu_backend: 'cuda' | 'vulkan' | 'cpu'
}

function draftFrom(config: AmaaraConfig | undefined): Draft {
  return {
    amaara_base_url: config?.amaara_base_url ?? '',
    default_model: config?.default_model ?? '',
    use_auto_router: config?.use_auto_router ?? false,
    byok: config?.byok ?? false,
    local_llama_url: config?.local_llama_url ?? '',
    engine_url: config?.engine_url ?? '',
    sd_server_url: config?.sd_server_url ?? '',
    sd_binary_path: config?.sd_binary_path ?? '',
    sd_models_dir: config?.sd_models_dir ?? '',
    sd_gpu_backend: config?.sd_gpu_backend ?? 'cuda',
  }
}

const labelStyle = {
  display: 'flex',
  flexDirection: 'column',
  gap: 4,
  fontSize: 12,
  color: 'var(--text-muted)',
} as const

export function SettingsView() {
  const config = useStore((s) => s.config)
  const harnesses = useStore((s) => s.harnesses) ?? []
  const session = useStore((s) => s.session)
  const saveApiKey = useStore((s) => s.saveApiKey)
  const clearApiKey = useStore((s) => s.clearApiKey)
  const setHarness = useStore((s) => s.setHarness)
  const setTheme = useStore((s) => s.setTheme)
  const setDensity = useStore((s) => s.setDensity)
  const setShareAnalytics = useStore((s) => s.setShareAnalytics)
  const saveConfig = useStore((s) => s.saveConfig)

  const [keyInput, setKeyInput] = useState('')
  const [packaging, setPackaging] = useState(false)
  const [feedbackPath, setFeedbackPath] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const hasApiKey = useStore((s) => s.has_api_key) ?? false

  const [draft, setDraft] = useState<Draft>(() => draftFrom(config))
  // Re-seed the draft when the persisted config changes (first load / save).
  useEffect(() => {
    if (config) setDraft(draftFrom(config))
  }, [config])

  const dirty =
    !!config &&
    (draft.amaara_base_url !== config.amaara_base_url ||
      draft.default_model !== config.default_model ||
      draft.use_auto_router !== config.use_auto_router ||
      draft.byok !== config.byok ||
      draft.local_llama_url !== config.local_llama_url ||
      draft.engine_url !== config.engine_url ||
      (draft.sd_server_url || null) !== (config.sd_server_url ?? null) ||
      draft.sd_binary_path !== config.sd_binary_path ||
      draft.sd_models_dir !== config.sd_models_dir ||
      draft.sd_gpu_backend !== config.sd_gpu_backend)

  const saveDraft = async () => {
    if (!config) return
    setSaving(true)
    try {
      await saveConfig({
        ...config,
        amaara_base_url: draft.amaara_base_url,
        default_model: draft.default_model,
        use_auto_router: draft.use_auto_router,
        byok: draft.byok,
        local_llama_url: draft.local_llama_url,
        engine_url: draft.engine_url,
        sd_server_url: draft.sd_server_url || null,
        sd_binary_path: draft.sd_binary_path,
        sd_models_dir: draft.sd_models_dir,
        sd_gpu_backend: draft.sd_gpu_backend,
      })
    } catch {
      setDraft(draftFrom(config)) // roll back on failure; status strip shows it
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="entry-main__scroll-inner entry-main__scroll-inner--narrow">
      {/* Amaara Cloud */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Amaara Cloud</h2>
          <div className="entry-section__actions">
            <span
              style={{
                fontFamily: 'var(--mono)',
                fontSize: 10,
                color: hasApiKey ? 'var(--green)' : 'var(--amber)',
              }}
            >
              {hasApiKey ? '● connected' : '● key missing'}
            </span>
          </div>
        </div>
        <div className="list-card">
          <div
            className="list-card__body"
            style={{display: 'flex', flexDirection: 'column', gap: 10, padding: 14}}
          >
            <label style={labelStyle}>
              Endpoint
              <input
                className="input input-sm mono"
                value={draft.amaara_base_url}
                onChange={(e) => setDraft({...draft, amaara_base_url: e.target.value})}
                placeholder="http://localhost:8000"
              />
            </label>
            <label style={labelStyle}>
              Default model
              <input
                className="input input-sm mono"
                value={draft.default_model}
                onChange={(e) => setDraft({...draft, default_model: e.target.value})}
                placeholder="amaara/auto"
              />
            </label>
            <div style={{display: 'flex', gap: 16}}>
              <label
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 6,
                  fontSize: 12,
                  color: 'var(--text-muted)',
                  cursor: 'pointer',
                }}
              >
                <input
                  type="checkbox"
                  checked={draft.use_auto_router}
                  onChange={(e) => setDraft({...draft, use_auto_router: e.target.checked})}
                />
                Use amaara/auto router
              </label>
              <label
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 6,
                  fontSize: 12,
                  color: 'var(--text-muted)',
                  cursor: 'pointer',
                }}
              >
                <input
                  type="checkbox"
                  checked={draft.byok}
                  onChange={(e) => setDraft({...draft, byok: e.target.checked})}
                />
                BYOK to underlying providers
              </label>
            </div>
          </div>
        </div>

        <div className="list-card" style={{marginTop: 12}}>
          <div className="list-card__body">
            {!hasApiKey && (
              <form
                onSubmit={async (e) => {
                  e.preventDefault()
                  if (!keyInput.trim()) return
                  await saveApiKey(keyInput.trim())
                  setKeyInput('')
                }}
                style={{display: 'flex', gap: 8, padding: 12}}
              >
                <input
                  className="input input-sm"
                  placeholder="amaara-…"
                  value={keyInput}
                  onChange={(e) => setKeyInput(e.target.value)}
                  type="password"
                  style={{flex: 1}}
                  autoFocus
                  aria-label="Amaara API key"
                />
                <button
                  type="submit"
                  className="btn btn-primary btn-sm"
                  disabled={!keyInput.trim()}
                >
                  Save
                </button>
              </form>
            )}
            {hasApiKey && (
              <div className="list-card__row">
                <span className="list-card__row-title">
                  <span style={{fontFamily: 'var(--mono)'}}>amaara-••••••••</span>
                </span>
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={async () => {
                    await clearApiKey()
                    setKeyInput('')
                  }}
                >
                  Clear
                </button>
              </div>
            )}
          </div>
        </div>
        <p style={{marginTop: 8, fontSize: 11, color: 'var(--text-faint)', lineHeight: 1.6}}>
          The key is stored locally in the OS keychain. Used only for cloud renders and model
          catalog sync. Clear it any time.
        </p>
      </section>

      {/* Local models */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Local Models</h2>
          <div className="entry-section__actions">
            <span style={{color: 'var(--text-faint)', fontSize: 11}}>llama.cpp + sd-server</span>
          </div>
        </div>
        <div className="list-card">
          <div
            className="list-card__body"
            style={{display: 'flex', flexDirection: 'column', gap: 10, padding: 14}}
          >
            <label style={labelStyle}>
              LLM (llama.cpp server) URL
              <input
                className="input input-sm mono"
                value={draft.local_llama_url}
                onChange={(e) => setDraft({...draft, local_llama_url: e.target.value})}
                placeholder="http://localhost:8080"
              />
            </label>
            <label style={labelStyle}>
              Amaara Engine (local proxy) URL
              <input
                className="input input-sm mono"
                value={draft.engine_url}
                onChange={(e) => setDraft({...draft, engine_url: e.target.value})}
                placeholder="http://127.0.0.1:7685"
              />
            </label>
            <label style={labelStyle}>
              Image (sd-server) URL — empty = managed (auto-started)
              <input
                className="input input-sm mono"
                value={draft.sd_server_url}
                onChange={(e) => setDraft({...draft, sd_server_url: e.target.value})}
                placeholder="(managed)"
              />
            </label>
            <div style={{display: 'flex', gap: 8}}>
              <label style={{...labelStyle, flex: 2}}>
                sd-server binary
                <input
                  className="input input-sm mono"
                  value={draft.sd_binary_path}
                  onChange={(e) => setDraft({...draft, sd_binary_path: e.target.value})}
                  placeholder="…/binaries/sd-server.exe"
                />
              </label>
              <label style={{...labelStyle, flex: 2}}>
                Models dir
                <input
                  className="input input-sm mono"
                  value={draft.sd_models_dir}
                  onChange={(e) => setDraft({...draft, sd_models_dir: e.target.value})}
                  placeholder="models"
                />
              </label>
            </div>
            <div style={{display: 'flex', alignItems: 'center', gap: 10}}>
              <span style={{fontSize: 12, color: 'var(--text-muted)'}}>GPU backend</span>
              <div className="seg" role="radiogroup" aria-label="GPU backend">
                {(['cuda', 'vulkan', 'cpu'] as const).map((b) => (
                  <button
                    key={b}
                    type="button"
                    role="radio"
                    aria-checked={draft.sd_gpu_backend === b}
                    className={draft.sd_gpu_backend === b ? 'is-active' : ''}
                    onClick={() => setDraft({...draft, sd_gpu_backend: b})}
                  >
                    {b}
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>
        <p style={{marginTop: 8, fontSize: 11, color: 'var(--text-faint)', lineHeight: 1.6}}>
          The GPU flavor matches the sd-server binary build (download-on-first-run). The binary path
          and models dir default under the app-data directory.
        </p>
      </section>

      {dirty && (
        <div style={{display: 'flex', gap: 8, marginBottom: 24}}>
          <button
            className="btn btn-primary btn-sm"
            disabled={saving}
            onClick={() => void saveDraft()}
          >
            {saving ? 'Saving…' : 'Save changes'}
          </button>
          <button className="btn btn-ghost btn-sm" onClick={() => setDraft(draftFrom(config))}>
            Discard
          </button>
        </div>
      )}

      {/* Harness */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Harness</h2>
          <div className="entry-section__actions">
            <span style={{color: 'var(--text-faint)', fontSize: 11}}>
              the CLI the studio spawns
            </span>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {harnesses.map((h) => {
              const isActive = session?.harness === h.id
              return (
                <button
                  key={h.id}
                  type="button"
                  className={`row ${isActive ? 'is-active' : ''}`}
                  onClick={() => void setHarness(h.id)}
                  disabled={!h.available}
                  title={harnessHint(h)}
                >
                  <span
                    aria-hidden
                    className={
                      h.available ? (isActive ? 'text-ok' : 'text-ink-faint') : 'text-ink-faint'
                    }
                    style={{fontSize: 10}}
                  >
                    ●
                  </span>
                  <span style={{flex: 1, minWidth: 0}} className="truncate">
                    {h.label}
                  </span>
                  <span className="list-card__row-meta">
                    {h.available ? (h.version ?? 'installed') : 'not installed'}
                  </span>
                </button>
              )
            })}
          </div>
        </div>
      </section>

      {/* Appearance */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Appearance</h2>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {(['light', 'dark'] as const).map((t) => (
              <button
                key={t}
                type="button"
                className={`row ${config?.theme === t ? 'is-active' : ''}`}
                onClick={() => void setTheme(t)}
              >
                <span aria-hidden style={{fontSize: 12, width: 16, textAlign: 'center'}}>
                  {t === 'light' ? '☀' : '☾'}
                </span>
                <span style={{flex: 1, minWidth: 0, textAlign: 'left'}}>
                  {t === 'light' ? 'Light' : 'Dark'}
                </span>
                {config?.theme === t && (
                  <span style={{color: 'var(--brand)', fontSize: 11}}>● active</span>
                )}
              </button>
            ))}
          </div>
        </div>
        <div className="list-card" style={{marginTop: 12}}>
          <div className="list-card__body">
            <div style={{padding: '10px 14px 0', fontSize: 11, color: 'var(--text-faint)'}}>
              density
            </div>
            {(['comfortable', 'compact'] as const).map((d) => (
              <button
                key={d}
                type="button"
                className={`row ${(config?.density ?? 'comfortable') === d ? 'is-active' : ''}`}
                onClick={() => void setDensity(d)}
              >
                <span aria-hidden style={{fontSize: 12, width: 16, textAlign: 'center'}}>
                  {d === 'compact' ? '≡' : '☰'}
                </span>
                <span style={{flex: 1, minWidth: 0, textAlign: 'left'}}>
                  {d === 'compact' ? 'Compact' : 'Comfortable'}
                  {d === 'compact' && (
                    <span style={{color: 'var(--text-faint)', marginLeft: 6, fontSize: 11}}>
                      tighter spacing for laptops
                    </span>
                  )}
                </span>
                {(config?.density ?? 'comfortable') === d && (
                  <span style={{color: 'var(--brand)', fontSize: 11}}>● active</span>
                )}
              </button>
            ))}
          </div>
        </div>
      </section>

      {/* Privacy */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Privacy</h2>
          <div className="entry-section__actions">
            <span style={{color: 'var(--text-faint)', fontSize: 11}}>
              anonymous usage telemetry
            </span>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {([false, true] as const).map((on) => (
              <button
                key={String(on)}
                type="button"
                className={`row ${config?.share_analytics === on ? 'is-active' : ''}`}
                onClick={() => void setShareAnalytics(on)}
              >
                <span aria-hidden style={{fontSize: 12, width: 16, textAlign: 'center'}}>
                  {on ? '●' : '○'}
                </span>
                <span style={{flex: 1, minWidth: 0, textAlign: 'left'}}>
                  {on ? 'Enabled' : 'Disabled'}
                </span>
                {config?.share_analytics === on && (
                  <span style={{color: 'var(--brand)', fontSize: 11}}>● active</span>
                )}
              </button>
            ))}
          </div>
        </div>
        <p style={{marginTop: 8, fontSize: 11, color: 'var(--text-faint)', lineHeight: 1.6}}>
          Off by default. When disabled, renders run with HyperFrames' telemetry disabled; enable it
          to share anonymous usage with the team.
        </p>
      </section>

      {/* About */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">About</h2>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            <div className="list-card__row">
              <span className="list-card__row-title">Amaara Studio</span>
              <span className="list-card__row-meta">v0.1.0</span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">Default agent</span>
              <span className="list-card__row-meta">{session?.harness ?? '—'}</span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">Feedback</span>
              <button
                type="button"
                className="btn btn-sm"
                disabled={packaging}
                onClick={async () => {
                  setPackaging(true)
                  try {
                    const p = await Commands.packageFeedback()
                    setFeedbackPath(p)
                  } catch (e) {
                    setFeedbackPath(`failed: ${String(e)}`)
                  } finally {
                    setPackaging(false)
                  }
                }}
              >
                {packaging ? 'Packaging…' : 'Package logs'}
              </button>
            </div>
            {feedbackPath && (
              <div style={{padding: '6px 10px', fontSize: 11, color: 'var(--text-faint)'}}>
                {feedbackPath.startsWith('failed:') ? (
                  feedbackPath
                ) : (
                  <button
                    type="button"
                    className="btn btn-ghost btn-sm"
                    onClick={() => void Commands.revealInFolder(feedbackPath)}
                  >
                    Reveal {feedbackPath.split(/[\\/]/).pop()}
                  </button>
                )}
              </div>
            )}
          </div>
        </div>
      </section>
    </div>
  )
}
