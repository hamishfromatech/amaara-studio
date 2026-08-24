/**
 * Navya Studio shell — state-driven (production wiring).
 *
 * Every panel reads from the Zustand store, which is fed by the Rust core
 * through #[tauri::command] + studio://event. Pickers call the real commands;
 * the chat renders real streamed events; the status strip reflects real
 * sidecar health. No hardcoded mock content.
 */

import { useEffect, useState } from "react";
import { useStore } from "../lib/store";
import { Commands } from "../lib/invoke";

// --- TopBar ---
function TopBar() {
  const session = useStore((s) => s.session);
  const config = useStore((s) => s.config);
  const projects = useStore((s) => s.projects) ?? [];
  const models = useStore((s) => s.models) ?? [];
  const harnesses = useStore((s) => s.harnesses) ?? [];
  const setModel = useStore((s) => s.setModel);
  const setSource = useStore((s) => s.setSource);
  const setHarness = useStore((s) => s.setHarness);
  const openProject = useStore((s) => s.openProject);
  const render = useStore((s) => s.render);
  const abort = useStore((s) => s.abort);

  if (!session || !config) return null;
  const current = projects.find((p) => p.id === session.current_project_id);

  return (
    <header className="flex items-center justify-between bg-studio-900 border-b border-studio-800 px-4 py-2 text-slate-100">
      <div className="flex items-center gap-4">
        <span className="font-bold text-accent">⬢ Navya Studio</span>
        <select
          className="bg-studio-800 border-none rounded px-2 py-1 text-sm"
          value={session.current_project_id ?? ""}
          onChange={(e) => e.target.value && void openProject(e.target.value)}
        >
          <option value="" disabled>
            {current ? current.name : "No project open"}
          </option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </div>

      <div className="flex items-center gap-3 text-sm">
        <button
          className="px-3 py-1 bg-studio-800 hover:bg-studio-700 rounded"
          onClick={() => void render("high")}
        >
          ⤓ Render
        </button>
        <button
          className="px-3 py-1 bg-studio-800 hover:bg-studio-700 rounded"
          onClick={() => void abort()}
        >
          ◐ Stop
        </button>

        <select
          className="bg-studio-800 border-none rounded px-2 py-1"
          value={session.harness}
          onChange={(e) => void setHarness(e.target.value)}
          title="Harness"
        >
          {harnesses.map((h) => (
            <option key={h.id} value={h.id}>
              {h.label} {h.available ? "" : "(not installed)"}
            </option>
          ))}
        </select>

        <select
          className="bg-studio-800 border-none rounded px-2 py-1"
          value={session.model}
          onChange={(e) => void setModel(e.target.value)}
          title="Model"
        >
          {models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name} {m.kind !== "chat" ? `(${m.kind})` : ""}
            </option>
          ))}
        </select>

        <div className="flex items-center gap-1">
          <button
            className={`px-2 py-1 rounded ${session.source === "cloud" ? "bg-accent text-white" : "bg-studio-800"}`}
            onClick={() => void setSource("cloud")}
          >
            ● Cloud
          </button>
          <button
            className={`px-2 py-1 rounded ${session.source === "local" ? "bg-accent text-white" : "bg-studio-800"}`}
            onClick={() => void setSource("local")}
          >
            ● Local
          </button>
        </div>
      </div>
    </header>
  );
}

// --- LeftRail ---
function LeftRail() {
  const projects = useStore((s) => s.projects) ?? [];
  const models = useStore((s) => s.models) ?? [];
  const session = useStore((s) => s.session);
  const newProject = useStore((s) => s.newProject);
  const openProject = useStore((s) => s.openProject);
  const setModel = useStore((s) => s.setModel);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [dir, setDir] = useState("");

  return (
    <aside className="w-60 border-r border-studio-800 bg-studio-950 text-slate-200 p-3 overflow-y-auto">
      <div className="mb-4">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">PROJECTS</h3>
        <div className="space-y-1 text-sm">
          {projects.length === 0 && <div className="text-slate-500 text-xs">No projects yet.</div>}
          {projects.map((p) => (
            <button
              key={p.id}
              onClick={() => void openProject(p.id)}
              className={`w-full text-left px-2 py-1 rounded ${
                session?.current_project_id === p.id ? "bg-studio-800 text-accent" : "hover:bg-studio-800/50"
              }`}
            >
              {p.name}
            </button>
          ))}
        </div>

        {creating ? (
          <div className="mt-2 space-y-1">
            <input
              className="w-full bg-studio-800 rounded px-2 py-1 text-sm"
              placeholder="Project name"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <input
              className="w-full bg-studio-800 rounded px-2 py-1 text-sm"
              placeholder="Directory"
              value={dir}
              onChange={(e) => setDir(e.target.value)}
            />
            <div className="flex gap-1">
              <button
                className="px-2 py-1 bg-accent text-white rounded text-xs"
                onClick={async () => {
                  if (name && dir) {
                    await newProject(name, dir);
                    setName("");
                    setDir("");
                    setCreating(false);
                  }
                }}
              >
                Create
              </button>
              <button className="px-2 py-1 bg-studio-800 rounded text-xs" onClick={() => setCreating(false)}>
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <button className="mt-2 text-xs text-slate-400 hover:text-accent" onClick={() => setCreating(true)}>
            + New project
          </button>
        )}
      </div>

      <div className="mb-4">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">MODELS</h3>
        <div className="space-y-1 text-sm">
          <div className="text-xs text-slate-500">Cloud (Navya)</div>
          {models.filter((m) => m.source === "cloud").map((m) => (
            <button
              key={m.id}
              onClick={() => void setModel(m.id)}
              className={`w-full text-left pl-2 ${m.active ? "text-accent" : "text-slate-300 hover:text-accent"}`}
            >
              {m.active ? "● " : "○ "}
              {m.name}
            </button>
          ))}
          <div className="text-xs text-slate-500 mt-2">Local</div>
          {models.filter((m) => m.source === "local").map((m) => (
            <button
              key={m.id}
              onClick={() => void setModel(m.id)}
              className={`w-full text-left pl-2 ${m.active ? "text-accent" : "text-slate-300 hover:text-accent"}`}
            >
              {m.active ? "● " : "○ "}
              {m.name}
            </button>
          ))}
        </div>
      </div>
    </aside>
  );
}

// --- ChatView ---
function ChatView() {
  const chat = useStore((s) => s.chat);
  const session = useStore((s) => s.session);
  const sendPrompt = useStore((s) => s.sendPrompt);
  const steer = useStore((s) => s.steer);
  const [text, setText] = useState("");

  const send = (mode: string) => {
    if (!text.trim()) return;
    void sendPrompt(text, mode);
    setText("");
  };

  return (
    <main className="flex-1 flex flex-col bg-studio-900 text-slate-100">
      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {chat.length === 0 && (
          <div className="text-slate-500 text-sm text-center mt-8">
            {session?.current_project_id
              ? "Describe what you want to make. The agent will author the composition and render it."
              : "Create or open a project to start chatting with the agent."}
          </div>
        )}
        {chat.map((m) => (
          <div key={m.id} className="space-y-1">
            <div className="text-xs text-slate-400">
              {m.role === "you" ? "you" : m.role === "agent" ? `agent · ${session?.harness ?? ""}` : "system"}
              {m.status === "thinking" && " · thinking…"}
              {m.status === "error" && " · error"}
              {m.status === "done" && " · done"}
            </div>
            <p className={`text-sm ${m.status === "error" ? "text-red-400" : "text-slate-200"}`}>
              {m.content || (m.status === "thinking" ? "…" : "")}
            </p>
          </div>
        ))}
      </div>

      <div className="border-t border-studio-800 p-4 bg-studio-950">
        <textarea
          className="w-full bg-studio-800 border border-studio-700 rounded p-2 text-sm resize-none focus:outline-none focus:border-accent"
          placeholder={
            session?.current_project_id ? "Reply to agent…" : "Open a project to enable the agent…"
          }
          rows={3}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) send("normal");
          }}
        />
        <div className="mt-2 flex gap-2 text-xs text-slate-400">
          <button className="px-2 py-1 bg-studio-800 hover:bg-studio-700 rounded" onClick={() => send("normal")}>
            ▶ Send
          </button>
          <button className="px-2 py-1 bg-studio-800 hover:bg-studio-700 rounded" onClick={() => steer(text)}>
            ⚙ steer
          </button>
        </div>
      </div>
    </main>
  );
}

// --- RightRail ---
function RightRail() {
  const renders = useStore((s) => s.renders);
  const cancelRender = useStore((s) => s.cancelRender);

  return (
    <aside className="w-72 border-l border-studio-800 bg-studio-950 text-slate-200 p-3 overflow-y-auto">
      <div className="mb-4">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">RENDER QUEUE</h3>
        <div className="space-y-1 text-sm">
          {renders.length === 0 && <div className="text-slate-500 text-xs">No renders yet.</div>}
          {renders.map((r) => (
            <div key={r.job_id} className="flex items-center justify-between bg-studio-800/50 p-2 rounded">
              <span>
                {r.status === "running" ? "▶" : r.status === "done" ? "✓" : r.status === "failed" ? "✗" : "○"}{" "}
                {r.job_id.slice(0, 8)} · {r.quality}
              </span>
              <div className="flex gap-1 text-xs">
                {r.status === "done" && r.output_path && (
                  <button
                    className="px-1 bg-studio-700 rounded"
                    onClick={() => {
                      void Commands.revealInFolder(r.output_path!);
                    }}
                  >
                    reveal
                  </button>
                )}
                {r.status === "running" && (
                  <button className="px-1 bg-studio-700 rounded" onClick={() => void cancelRender(r.job_id)}>
                    ✕
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
      </div>

      <InspectorPanel />
    </aside>
  );
}

function InspectorPanel() {
  const session = useStore((s) => s.session);
  const config = useStore((s) => s.config);
  const projects = useStore((s) => s.projects) ?? [];
  if (!session || !config) return null;
  const current = projects.find((p) => p.id === session.current_project_id);

  return (
    <div>
      <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">INSPECTOR</h3>
      <div className="text-sm space-y-1 bg-studio-800/50 p-2 rounded">
        <div className="font-medium">▸ {current ? current.name : "No project"}</div>
        {current && (
          <>
            <div className="pl-2 text-xs text-slate-400">{current.dir}</div>
            <div className="pl-2 text-xs text-slate-400">harness: {current.harness}</div>
            <div className="pl-2 text-xs text-slate-400">model: {current.model}</div>
          </>
        )}
        <div className="mt-2 pt-2 border-t border-studio-700">
          <div className="text-xs font-semibold mb-1">─ session ──</div>
          <div className="pl-2 text-xs text-slate-400">source: {session.source}</div>
          <div className="pl-2 text-xs text-slate-400">model: {session.model}</div>
        </div>
      </div>
    </div>
  );
}

// --- StatusStrip ---
function StatusStrip() {
  const sidecars = useStore((s) => s.sidecars) ?? [];
  const error = useStore((s) => s.error);
  const renderCount = useStore((s) => s.render_count);

  const dot = (status: string) => (status === "running" ? "text-accent" : status === "exited" ? "text-red-400" : "text-slate-500");

  return (
    <footer className="bg-studio-950 border-t border-studio-800 px-4 py-1 text-xs flex items-center gap-4 text-slate-300">
      {sidecars.map((sc) => (
        <span key={sc.name} className={dot(sc.status)} title={sc.detail ?? sc.status}>
          ● {sc.name} {sc.status}
        </span>
      ))}
      <span className="ml-auto text-slate-400">{renderCount} render(s)</span>
      {error && <span className="text-red-400 truncate max-w-xs">⚠ {error}</span>}
    </footer>
  );
}

// --- TabBar + tab state ---
function TabBar({ tab, setTab }: { tab: string; setTab: (t: string) => void }) {
  const tabs = ["Chat", "Timeline", "Assets", "Renders"];
  return (
    <div className="flex border-b border-studio-800 bg-studio-950 text-sm">
      {tabs.map((t) => (
        <button
          key={t}
          className={`px-4 py-2 border-r border-studio-800 ${tab === t ? "bg-studio-800 text-accent" : "text-slate-300 hover:bg-studio-800/50"}`}
          onClick={() => setTab(t)}
        >
          {t}
        </button>
      ))}
    </div>
  );
}

// --- StudioShell (top-level) ---
export function StudioShell() {
  const [tab, setTab] = useState("Chat");
  const refreshRenders = useStore((s) => s.refreshRenders);

  useEffect(() => {
    void refreshRenders();
  }, [refreshRenders]);

  return (
    <div className="flex flex-col h-screen bg-studio-900 text-slate-100">
      <TopBar />
      <TabBar tab={tab} setTab={setTab} />
      <div className="flex flex-1 overflow-hidden">
        <LeftRail />
        {tab === "Chat" && <ChatView />}
        {tab === "Timeline" && <PlaceholderTab name="Timeline" hint="Open a HyperFrames composition to see the preview + tracks." />}
        {tab === "Assets" && <PlaceholderTab name="Assets" hint="Generated images and imported media appear here." />}
        {tab === "Renders" && <PlaceholderTab name="Renders" hint="The render queue is in the right rail." />}
        <RightRail />
      </div>
      <StatusStrip />
    </div>
  );
}

function PlaceholderTab({ name, hint }: { name: string; hint: string }) {
  return (
    <main className="flex-1 flex items-center justify-center bg-studio-900 text-slate-500 text-sm">
      <div className="text-center">
        <div className="font-medium text-slate-300 mb-1">{name}</div>
        <div>{hint}</div>
      </div>
    </main>
  );
}