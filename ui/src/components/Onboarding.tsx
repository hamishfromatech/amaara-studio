/**
 * Onboarding (first launch) — production wiring.
 *
 * Per design.md §8: first launch shows Navya key entry OR start-local, plus a
 * harness picker. The agent runs the intent interview in-chat later (no wizard).
 * Here the "Connect Navya Cloud" flow calls the real set_api_key command; the
 * harness picker calls set_harness; "Start with a local model" sets source=local.
 */

import { useState } from "react";
import { useStore } from "../lib/store";
import { harnessHint } from "../lib/invoke";

export function OnboardingScreen() {
  const saveApiKey = useStore((s) => s.saveApiKey);
  const setSource = useStore((s) => s.setSource);
  const setHarness = useStore((s) => s.setHarness);
  const harnesses = useStore((s) => s.harnesses) ?? [];
  const session = useStore((s) => s.session);
  const newProject = useStore((s) => s.newProject);

  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const connectCloud = async () => {
    setBusy(true);
    setErr(null);
    try {
      if (key.trim()) await saveApiKey(key.trim());
      await setSource("cloud");
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const startLocal = async () => {
    setBusy(true);
    setErr(null);
    try {
      await setSource("local");
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const startBlankProject = async () => {
    setBusy(true);
    try {
      await newProject("My first video", ".");
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-screen flex-col items-center justify-center bg-canvas p-8 text-ink">
      <h1 className="mb-1.5 text-2xl font-bold tracking-tight text-ink-strong">⬢ Navya Studio</h1>
      <p className="mb-8 max-w-md text-center text-sm text-ink-muted">
        A content-creation studio powered by AI. Direct an agent. Watch it make. Render to video.
      </p>

      <div className="grid w-full max-w-2xl grid-cols-2 gap-4">
        <div className="rounded-xl border border-line-soft bg-panel p-5 shadow-[var(--shadow-xs)] transition-colors hover:border-line">
          <h3 className="mb-1 text-sm font-semibold text-ink-strong">Connect Navya Cloud</h3>
          <p className="mb-3 text-[13px] text-ink-muted">Paste your API key to use hosted models.</p>
          <input
            type="password"
            className="input mb-3"
            placeholder="Paste your Navya API key"
            value={key}
            onChange={(e) => setKey(e.target.value)}
          />
          <button className="btn btn-primary w-full" disabled={busy} onClick={connectCloud}>
            {busy ? "Connecting…" : "Connect →"}
          </button>
        </div>

        <div className="flex flex-col rounded-xl border border-line-soft bg-panel p-5 shadow-[var(--shadow-xs)] transition-colors hover:border-line">
          <h3 className="mb-1 text-sm font-semibold text-ink-strong">Start with a local model</h3>
          <p className="mb-3 flex-1 text-[13px] text-ink-muted">Run fully offline with llama.cpp + sd-server.</p>
          <button className="btn w-full" disabled={busy} onClick={startLocal}>
            Use local →
          </button>
        </div>
      </div>

      <div className="mt-5 text-center text-[13px] text-ink-muted">
        …or{" "}
        <button
          className="font-medium text-ink-strong underline decoration-line underline-offset-2 transition-colors hover:decoration-ink-strong"
          onClick={startBlankProject}
          disabled={busy}
        >
          start a blank project
        </button>
      </div>

      <div className="mt-8 flex max-w-2xl flex-wrap items-center justify-center gap-2">
        <span className="text-xs text-ink-soft">Pick a harness</span>
        {harnesses.map((h) => (
          <button
            key={h.id}
            onClick={() => void setHarness(h.id)}
            className={`chip mono ${session?.harness === h.id ? "is-active" : ""}`}
            disabled={!h.available}
            title={h.available ? "installed" : "not installed on PATH"}
          >
            <span className={`text-[9px] ${h.available ? (session?.harness === h.id ? "" : "text-ok") : "text-ink-faint"}`}>
              ●
            </span>
            {h.label}
          </button>
        ))}
      </div>

      {err && <div className="mt-4 text-[13px] text-danger">⚠ {err}</div>}
    </div>
  );
}
