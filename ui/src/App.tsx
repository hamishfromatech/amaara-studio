/**
 * Navya Studio App — production wiring.
 *
 * Loads real state from the Rust core on mount, subscribes to the event
 * stream, and renders either the onboarding screen (no API key / no project)
 * or the full studio shell. No hardcoded mock data — everything comes from
 * the store, which is fed by #[tauri::command] + studio://event.
 */

import { useEffect } from "react";
import { useStore } from "./lib/store";
import { OnboardingScreen } from "./components/Onboarding";
import { StudioShell } from "./components/Shell";

export default function App() {
  const loading = useStore((s) => s.loading);
  const initialized = useStore((s) => s.initialized);
  const hasApiKey = useStore((s) => s.has_api_key);
  const projectCount = useStore((s) => (s.projects ?? []).length);
  const loadState = useStore((s) => s.loadState);

  useEffect(() => {
    void loadState();
  }, [loadState]);

  if (!initialized || loading) {
    return (
      <div className="flex items-center justify-center h-screen bg-studio-900 text-slate-300">
        <div className="text-center">
          <div className="text-2xl font-bold text-accent mb-2">⬢ Navya Studio</div>
          <div className="text-sm animate-pulse">Starting up…</div>
        </div>
      </div>
    );
  }

  // First-run onboarding: no API key and no projects yet.
  if (!hasApiKey && projectCount === 0) {
    return <OnboardingScreen />;
  }

  return <StudioShell />;
}