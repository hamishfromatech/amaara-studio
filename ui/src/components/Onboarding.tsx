/**
 * Onboarding (first launch) — the cinematic first-run.
 *
 * Design: a full-bleed darkroom stage — projector glow, film grain (global),
 * Fraunces display headline, staggered reveal. Two doors: connect Amaara
 * Cloud (key entry) or run local (llama.cpp + sd-server), plus a quiet
 * "blank project" escape and the harness picker as designed chips.
 *
 * Wiring is untouched: set_api_key / set_source / set_harness / new_project.
 */

import {useState} from 'react'
import {useStore} from '../lib/store'
import {Logo} from './Logo'

export function OnboardingScreen() {
  const saveApiKey = useStore((s) => s.saveApiKey)
  const setSource = useStore((s) => s.setSource)
  const setHarness = useStore((s) => s.setHarness)
  const harnesses = useStore((s) => s.harnesses) ?? []
  const session = useStore((s) => s.session)
  const newProject = useStore((s) => s.newProject)

  const [key, setKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(null)

  const connectCloud = async () => {
    setBusy(true)
    setErr(null)
    try {
      if (key.trim()) await saveApiKey(key.trim())
      await setSource('cloud')
    } catch (e) {
      setErr(String(e))
    } finally {
      setBusy(false)
    }
  }

  const startLocal = async () => {
    setBusy(true)
    setErr(null)
    try {
      await setSource('local')
    } catch (e) {
      setErr(String(e))
    } finally {
      setBusy(false)
    }
  }

  const startBlankProject = async () => {
    setBusy(true)
    try {
      await newProject('My first video', '.')
    } catch (e) {
      setErr(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="onboard">
      <div className="onboard__stage">
        {/* Brand */}
        <div className="onboard__brand reveal">
          <Logo size={30} />
        </div>

        {/* Display headline */}
        <h1 className="onboard__title reveal reveal-1">
          The studio where
          <br />
          <em>an agent makes film.</em>
        </h1>
        <p className="onboard__sub reveal reveal-2">
          Direct it in plain language. Watch every step. Render to video — cloud or fully
          offline.
        </p>

        {/* Two doors */}
        <div className="onboard__doors reveal reveal-3">
          <div className="onboard__card">
            <div className="onboard__card-kicker">Hosted</div>
            <h2 className="onboard__card-title">Connect Amaara Cloud</h2>
            <p className="onboard__card-desc">
              Paste your API key for hosted models and the render farm.
            </p>
            <input
              type="password"
              className="input onboard__key"
              placeholder="amaara_…"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              aria-label="Amaara API key"
            />
            <button
              className="btn btn-primary onboard__cta"
              disabled={busy}
              onClick={connectCloud}
            >
              {busy ? 'Connecting…' : 'Connect'}
              <span aria-hidden>→</span>
            </button>
          </div>

          <div className="onboard__card onboard__card--alt">
            <div className="onboard__card-kicker">Offline</div>
            <h2 className="onboard__card-title">Start with a local model</h2>
            <p className="onboard__card-desc">
              llama.cpp for words, sd-server for pixels. Nothing leaves this machine.
            </p>
            <button
              className="btn onboard__cta"
              disabled={busy}
              onClick={startLocal}
            >
              Use local
              <span aria-hidden>→</span>
            </button>
          </div>
        </div>

        {/* Quiet escape */}
        <div className="onboard__blank reveal reveal-4">
          …or{' '}
          <button type="button" onClick={startBlankProject} disabled={busy}>
            start a blank project
          </button>
        </div>

        {/* Harness picker */}
        {harnesses.length > 0 && (
          <div className="onboard__harness reveal reveal-5">
            <span className="onboard__harness-label">Harness</span>
            {harnesses.map((h) => {
              const active = session?.harness === h.id
              return (
                <button
                  key={h.id}
                  type="button"
                  onClick={() => void setHarness(h.id)}
                  className={`chip mono ${active ? 'is-active' : ''}`}
                  disabled={!h.available}
                  title={h.available ? 'installed' : 'not installed on PATH'}
                >
                  <span
                    aria-hidden
                    className={h.available ? (active ? '' : 'text-ok') : 'text-ink-faint'}
                    style={{fontSize: 9, color: active ? 'var(--brand)' : undefined}}
                  >
                    ●
                  </span>
                  {h.label}
                </button>
              )
            })}
          </div>
        )}

        {err && (
          <div className="onboard__err reveal reveal-5" role="alert">
            <span aria-hidden>⚠</span> {err}
          </div>
        )}
      </div>
    </div>
  )
}