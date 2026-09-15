/**
 * SourcesView — where generations come from, and how each source is wired.
 *
 * Three real surfaces, all fed by live commands (no hardcoded state):
 *   Generation source — the Cloud/Local toggle the TopBar carries, with
 *                        what each means and whether it's ready.
 *   Amaara Cloud       — endpoint, key status, router/BYOK flags, and a
 *                       live connection test (list_cloud_models).
 *   Amaara Engine      — the local OpenAI-compatible proxy: URL, health
 *                       (detect_engine), and its model catalog.
 *   Local (sd-server) — llama.cpp URL, sd-server URL/binary/models dir/GPU
 *                       backend, and the live sidecar status.
 */

import {useEffect, useState} from 'react'
import {useStore} from '../lib/store'
import {Commands} from '../lib/invoke'

/** Small status pill: ok / warn / bad + label. */
function StatusPill({tone, label}: {tone: 'ok' | 'warn' | 'bad' | 'idle'; label: string}) {
  const color =
    tone === 'ok'
      ? 'var(--green)'
      : tone === 'warn'
        ? 'var(--amber)'
        : tone === 'bad'
          ? 'var(--red)'
          : 'var(--text-faint)'
  return (
    <span
      className="chip"
      style={{cursor: 'default', height: 22, fontSize: 11, opacity: 1}}
      title={label}
    >
      <span aria-hidden style={{fontSize: 9, color}}>
        ●
      </span>
      {label}
    </span>
  )
}

export function SourcesView() {
  const source = useStore((s) => s.session?.source ?? 'cloud')
  const setSource = useStore((s) => s.setSource)
  const hasApiKey = useStore((s) => s.has_api_key) ?? false
  const config = useStore((s) => s.config)
  const sidecars = useStore((s) => s.sidecars) ?? []

  // --- Amaara Cloud live test ------------------------------------------------
  const [cloudTest, setCloudTest] = useState<{
    state: 'idle' | 'testing' | 'ok' | 'fail'
    detail: string
  }>({
    state: 'idle',
    detail: '',
  })
  const testCloud = async () => {
    setCloudTest({state: 'testing', detail: 'reaching ' + (config?.amaara_base_url ?? 'cloud')})
    try {
      const models = await Commands.listCloudModels()
      setCloudTest({state: 'ok', detail: `${models.length} model(s) reachable`})
    } catch (e) {
      setCloudTest({state: 'fail', detail: String(e)})
    }
  }

  // --- Amaara Engine health ---------------------------------------------------
  const [engine, setEngine] = useState<{state: 'idle' | 'checking' | 'ok' | 'down'; count: number}>(
    {
      state: 'idle',
      count: 0,
    },
  )
  const checkEngine = async () => {
    setEngine({state: 'checking', count: 0})
    try {
      const models = await Commands.detectEngine()
      setEngine({state: models.length > 0 ? 'ok' : 'down', count: models.length})
    } catch {
      setEngine({state: 'down', count: 0})
    }
  }
  useEffect(() => {
    void checkEngine()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config?.engine_url])

  const sdSidecar = sidecars.find((s) => s.name.includes('sd'))

  return (
    <div className="entry-main__scroll-inner">
      {/* Generation source toggle */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Generation Source</h2>
          <div className="entry-section__actions">
            <span style={{color: 'var(--text-faint)', fontFamily: 'var(--mono)', fontSize: 10}}>
              current · {source}
            </span>
          </div>
        </div>

        <div style={{display: 'flex', gap: 12}}>
          <button
            type="button"
            className="list-card"
            onClick={() => void setSource('cloud')}
            style={{
              flex: 1,
              textAlign: 'left',
              padding: 14,
              cursor: 'pointer',
              borderColor: source === 'cloud' ? 'var(--border-selected)' : undefined,
              background:
                source === 'cloud'
                  ? 'color-mix(in srgb, var(--selected) 6%, var(--bg))'
                  : undefined,
            }}
            aria-pressed={source === 'cloud'}
          >
            <div style={{display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6}}>
              <span
                style={{
                  fontSize: 14,
                  color: source === 'cloud' ? 'var(--brand)' : 'var(--text-faint)',
                }}
              >
                ●
              </span>
              <span style={{fontSize: 14, fontWeight: 600, color: 'var(--text-strong)'}}>
                Cloud (Amaara)
              </span>
            </div>
            <p style={{margin: 0, fontSize: 12, color: 'var(--text-muted)', lineHeight: 1.6}}>
              Render on Amaara-managed GPUs. Requires an API key. Highest fidelity and the broadest
              model catalog.
            </p>
            <div
              style={{
                marginTop: 10,
                fontFamily: 'var(--mono)',
                fontSize: 10,
                color: 'var(--text-soft)',
              }}
            >
              {hasApiKey ? '✓ connected' : '✗ no api key'}
            </div>
          </button>

          <button
            type="button"
            className="list-card"
            onClick={() => void setSource('local')}
            style={{
              flex: 1,
              textAlign: 'left',
              padding: 14,
              cursor: 'pointer',
              borderColor: source === 'local' ? 'var(--border-selected)' : undefined,
              background:
                source === 'local'
                  ? 'color-mix(in srgb, var(--selected) 6%, var(--bg))'
                  : undefined,
            }}
            aria-pressed={source === 'local'}
          >
            <div style={{display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6}}>
              <span
                style={{
                  fontSize: 14,
                  color: source === 'local' ? 'var(--brand)' : 'var(--text-faint)',
                }}
              >
                ●
              </span>
              <span style={{fontSize: 14, fontWeight: 600, color: 'var(--text-strong)'}}>
                Local
              </span>
            </div>
            <p style={{margin: 0, fontSize: 12, color: 'var(--text-muted)', lineHeight: 1.6}}>
              Render on this machine via the SD.cpp sidecar. Lower fidelity, no API key, but private
              and offline-friendly.
            </p>
            <div
              style={{
                marginTop: 10,
                fontFamily: 'var(--mono)',
                fontSize: 10,
                color: 'var(--text-soft)',
              }}
            >
              {sdSidecar ? `sd-server · ${sdSidecar.status}` : 'sd-server · not started'}
            </div>
          </button>
        </div>
      </section>

      {/* Amaara Cloud */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Amaara Cloud</h2>
          <div className="entry-section__actions">
            {cloudTest.state === 'idle' && (
              <button className="entry-section__action" onClick={() => void testCloud()}>
                Test connection →
              </button>
            )}
            {cloudTest.state === 'testing' && <span className="shimmer-text">testing…</span>}
            {cloudTest.state === 'ok' && <StatusPill tone="ok" label={cloudTest.detail} />}
            {cloudTest.state === 'fail' && <StatusPill tone="bad" label="unreachable" />}
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            <div className="list-card__row">
              <span className="list-card__row-title">Endpoint</span>
              <span className="list-card__row-meta" title={config?.amaara_base_url}>
                {config?.amaara_base_url ?? '—'}
              </span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">API key</span>
              <span className={`list-card__row-meta ${hasApiKey ? 'text-ok' : 'text-warn'}`}>
                {hasApiKey ? '● set (keychain)' : '● missing'}
              </span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">Default model</span>
              <span className="list-card__row-meta">{config?.default_model ?? '—'}</span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">Auto router (amaara/auto)</span>
              <span className="list-card__row-meta">{config?.use_auto_router ? 'on' : 'off'}</span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">BYOK to underlying providers</span>
              <span className="list-card__row-meta">{config?.byok ? 'on' : 'off'}</span>
            </div>
          </div>
        </div>
        <p style={{marginTop: 8, fontSize: 11, color: 'var(--text-faint)', lineHeight: 1.6}}>
          Change the endpoint and flags in Settings → Amaara Cloud. The key lives in the OS keychain
          and is never written to disk.
        </p>
      </section>

      {/* Amaara Engine (local proxy) */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Amaara Engine (local proxy)</h2>
          <div className="entry-section__actions">
            <button className="entry-section__action" onClick={() => void checkEngine()}>
              {engine.state === 'checking' ? 'checking…' : 'Re-check →'}
            </button>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            <div className="list-card__row">
              <span className="list-card__row-title">URL</span>
              <span className="list-card__row-meta">{config?.engine_url ?? '—'}</span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">Health</span>
              <span className="list-card__row-meta">
                {engine.state === 'ok' && (
                  <span className="text-ok">● up · {engine.count} model(s)</span>
                )}
                {engine.state === 'down' && <span className="text-danger">● down</span>}
                {engine.state === 'checking' && <span className="shimmer-text">checking…</span>}
                {engine.state === 'idle' && <span className="text-ink-faint">not checked</span>}
              </span>
            </div>
          </div>
        </div>
        <p style={{marginTop: 8, fontSize: 11, color: 'var(--text-faint)', lineHeight: 1.6}}>
          OpenAI-compatible endpoint for locally served models. When up, its catalog joins the model
          picker under “Engine”.
        </p>
      </section>

      {/* Local (sd-server + llama.cpp) */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Local (sd-server + llama.cpp)</h2>
          <div className="entry-section__actions">
            {sdSidecar ? (
              <StatusPill
                tone={
                  sdSidecar.status === 'running'
                    ? 'ok'
                    : sdSidecar.status === 'exited'
                      ? 'bad'
                      : 'warn'
                }
                label={sdSidecar.status}
              />
            ) : (
              <StatusPill tone="idle" label="not started" />
            )}
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            <div className="list-card__row">
              <span className="list-card__row-title">llama.cpp server</span>
              <span className="list-card__row-meta">{config?.local_llama_url ?? '—'}</span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">sd-server URL</span>
              <span className="list-card__row-meta">
                {config?.sd_server_url ?? 'auto (managed)'}
              </span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">sd-server binary</span>
              <span className="list-card__row-meta" title={config?.sd_binary_path}>
                {config?.sd_binary_path ? config.sd_binary_path.split(/[\\/]/).pop() : '—'}
              </span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">Models dir</span>
              <span className="list-card__row-meta">{config?.sd_models_dir ?? '—'}</span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">GPU backend</span>
              <span className="list-card__row-meta">{config?.sd_gpu_backend ?? 'cuda'}</span>
            </div>
          </div>
        </div>
        <p style={{marginTop: 8, fontSize: 11, color: 'var(--text-faint)', lineHeight: 1.6}}>
          The sd-server sidecar starts lazily on the first local image (download-on-first-run with
          checksum). Paths and the GPU flavor are editable in Settings → Local models.
        </p>
      </section>
    </div>
  )
}
