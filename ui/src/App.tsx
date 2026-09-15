/**
 * Amaara Studio App — production wiring.
 *
 * Loads real state from the Rust core on mount, subscribes to the event
 * stream, and renders either the onboarding screen (no API key / no project)
 * or the full studio shell. No hardcoded mock data — everything comes from
 * the store, which is fed by #[tauri::command] + studio://event.
 */

import {useEffect} from 'react'
import {useStore} from './lib/store'
import {OnboardingScreen} from './components/Onboarding'
import {StudioShell} from './components/Shell'

export default function App() {
  const loading = useStore((s) => s.loading)
  const initialized = useStore((s) => s.initialized)
  const hasApiKey = useStore((s) => s.has_api_key)
  const projectCount = useStore((s) => (s.projects ?? []).length)
  const loadState = useStore((s) => s.loadState)

  useEffect(() => {
    void loadState()
  }, [loadState])

  if (!initialized || loading) {
    return (
      <div className="flex h-screen items-center justify-center bg-canvas text-ink">
        <div className="text-center">
          <div className="mb-1.5 text-xl font-bold tracking-tight text-ink-strong">
            ⬢ Amaara Studio
          </div>
          <div className="animate-pulse text-[13px] text-ink-muted">Starting up…</div>
        </div>
      </div>
    )
  }

  // First-run onboarding: no API key and no projects yet.
  if (!hasApiKey && projectCount === 0) {
    return <OnboardingScreen />
  }

  return <StudioShell />
}
