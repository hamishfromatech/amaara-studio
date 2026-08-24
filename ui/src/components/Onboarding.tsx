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
    <div className="flex flex-col items-center justify-center h-screen bg-studio-900 text-slate-100 p-8">
      <h1 className="text-3xl font-bold mb-2">⬢ Navya Studio</h1>
      <p className="text-center text-slate-300 mb-8 max-w-md">
        A content-creation studio powered by AI. Direct an agent. Watch it make. Render to video.
      </p>

      <div className="grid grid-cols-2 gap-6 max-w-2xl w-full">
        <div className="bg-studio-800 p-4 rounded-lg">
          <h3 className="font-semibold mb-2">Connect Navya Cloud</h3>
          <input
            type="password"
            className="w-full bg-studio-900 rounded px-2 py-1 text-sm mb-2 border border-studio-700"
            placeholder="Paste your Navya API key"
            value={key}
            onChange={(e) => setKey(e.target.value)}
          />
          <button
            className="w-full bg-accent hover:bg-accent/80 text-white py-2 rounded text-sm"
            disabled={busy}
            onClick={connectCloud}
          >
            {busy ? "Connecting…" : "Connect →"}
          </button>
        </div>

        <div className="bg-studio-800 p-4 rounded-lg">
          <h3 className="font-semibold mb-2">Start with a local model</h3>
          <p className="text-sm text-slate-400 mb-2">Run fully offline with llama.cpp + sd-server.</p>
          <button
            className="w-full bg-studio-700 hover:bg-studio-600 py-2 rounded text-sm"
            disabled={busy}
            onClick={startLocal}
          >
            Use local →
          </button>
        </div>
      </div>

      <div className="mt-6 text-center text-sm text-slate-400">
        …or{" "}
        <button className="text-accent hover:underline" onClick={startBlankProject} disabled={busy}>
          start a blank project
        </button>
      </div>

      <div className="mt-8 flex items-center gap-2 text-sm flex-wrap justify-center max-w-2xl">
        <span>Pick a harness:</span>
        {harnesses.map((h) => (
          <button
            key={h.id}
            onClick={() => void setHarness(h.id)}
            className={`font-mono px-2 py-1 rounded ${
              session?.harness === h.id ? "bg-accent text-white" : "bg-studio-800"
            } ${h.available ? "" : "opacity-50"}`}
            title={h.available ? "installed" : "not installed on PATH"}
          >
            {h.available ? "●" : "○"} {h.label}
          </button>
        ))}
      </div>

      {err && <div className="mt-4 text-red-400 text-sm">⚠ {err}</div>}
    </div>
  );
}