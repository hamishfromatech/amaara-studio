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
import {Logo} from './components/Logo'

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
      <div className="flex h-screen flex-col items-center justify-center gap-4 bg-canvas text-ink">
        <div className="animate-pulse">
          <Logo size={30} />
        </div>
        <div className="display text-[22px] font-medium tracking-tight text-ink-strong">
          Amaara Studio
        </div>
        <div className="shimmer-text font-mono text-[11px] tracking-[0.14em] text-ink-faint uppercase">
          Opening the darkroom…
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
