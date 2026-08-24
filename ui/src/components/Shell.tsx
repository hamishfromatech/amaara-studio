/**
 * Navya Studio UI Shell Components (Phase 6).
 *
 * Top bar, rails, tabs, chat — the design.md window shell rendered and wired.
 */

import React from "react";

// --- TopBar ---
export function TopBar() {
  return (
    <header className="flex items-center justify-between bg-studio-900 border-b border-studio-800 px-4 py-2 text-slate-100">
      <div className="flex items-center gap-4">
        <span className="font-bold text-accent">⬢ Navya Studio</span>
        <span className="text-sm text-slate-300">◴ black-holes-explainer ▾</span>
      </div>
      <div className="flex items-center gap-4 text-sm">
        <button className="px-3 py-1 bg-studio-800 hover:bg-studio-700 rounded">⤓ Render</button>
        <button className="px-3 py-1 bg-studio-800 hover:bg-studio-700 rounded">◐ Stop</button>
        <span>Harness: a-coder-cli ▾</span>
        <span>Model: navya/auto ▾</span>
        <span>Source: ● Cloud / ● Local</span>
      </div>
    </header>
  );
}

// --- LeftRail ---
export function LeftRail() {
  return (
    <aside className="w-60 border-r border-studio-800 bg-studio-950 text-slate-200 p-3 overflow-y-auto">
      <div className="mb-4">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">COMPOSITIONS</h3>
        <select className="w-full bg-studio-800 border-none rounded p-1 text-sm">
          <option>▾ black-holes-explainer</option>
        </select>
        <div className="mt-2 space-y-1 text-sm">
          <div className="flex items-center gap-2">
            <span className="text-accent">●</span> 30s cut <span className="ml-auto text-xs">✓</span>
          </div>
          <div className="flex items-center gap-2 text-slate-400">
            <span>○</span> 60s cut
          </div>
        </div>
      </div>

      <div className="mb-4">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">PROJECT</h3>
        <div className="space-y-1 text-sm font-mono bg-studio-800/50 p-2 rounded">
          <div>├ composition.html</div>
          <div>├ BRIEF.md</div>
          <div>├ STORYBOARD.md</div>
          <div>├ assets/</div>
          <div>│ └ img/</div>
          <div>└ renders/</div>
        </div>
      </div>

      <div className="mb-4">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">ASSETS</h3>
        <div className="grid grid-cols-3 gap-1">
          {[1, 2, 3].map((i) => (
            <div key={i} className="aspect-square bg-studio-800 rounded hover:bg-studio-700 cursor-pointer" />
          ))}
        </div>
      </div>

      <div>
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">MODELS</h3>
        <div className="space-y-1 text-sm">
          <div className="font-medium text-accent">▾ Cloud (Navya)</div>
          <div className="pl-2 text-slate-300">● navya/auto ✓</div>
          <div className="pl-2 text-slate-400">○ Qwen3-32B-TEE</div>
          <div className="font-medium mt-2 text-slate-300">▾ Local</div>
          <div className="pl-2 text-slate-400">○ llama3-8b (llama.cpp)</div>
        </div>
      </div>
    </aside>
  );
}

// --- ChatView (Center) ---
export function ChatView() {
  return (
    <main className="flex-1 flex flex-col bg-studio-900 text-slate-100">
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {/* You message */}
        <div className="space-y-2">
          <div className="text-xs text-slate-400">you · 14:02</div>
          <p className="text-sm">make a 30s faceless explainer about black holes, calm tone, navy palette</p>
        </div>

        {/* Agent response with tool cards */}
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-xs text-slate-400">
            <span>◷ agent · a-coder-cli · navya/auto · thinking…</span>
          </div>
          <p className="text-sm text-slate-300">Routing to /faceless-explainer. Running the intent interview…</p>

          {/* Tool card: write */}
          <div className="bg-studio-800/50 border border-studio-700 rounded p-2 space-y-1">
            <div className="text-xs font-mono text-accent">🛠 tool: write</div>
            <div className="text-sm">BRIEF.md · 612 bytes</div>
            <div className="text-xs text-slate-400">─ subject: black holes · tone: calm · palette: navy · dur: 30s</div>
          </div>

          {/* Tool card: generate_image */}
          <div className="bg-studio-800/50 border border-studio-700 rounded p-2 space-y-1">
            <div className="text-xs font-mono text-accent">🛠 tool: generate_image</div>
            <div className="text-sm">prompt: "event horizon, deep navy, minimal" ● Cloud</div>
            <div className="w-full bg-studio-700 h-2 rounded overflow-hidden">
              <div className="bg-accent h-full w-3/4 animate-pulse" />
            </div>
          </div>
        </div>
      </div>

      {/* Compose box */}
      <div className="border-t border-studio-800 p-4 bg-studio-950">
        <div className="flex gap-2">
          <textarea
            className="flex-1 bg-studio-800 border border-studio-700 rounded p-2 text-sm resize-none focus:outline-none focus:border-accent"
            placeholder="Reply to agent…"
            rows={3}
          />
        </div>
        <div className="mt-2 flex gap-2 text-xs text-slate-400">
          <button>⚙ steer</button>
          <button>follow-up</button>
          <button>attach image</button>
          <button>@asset</button>
        </div>
      </div>
    </main>
  );
}

// --- RightRail (Inspector + Render Queue) ---
export function RightRail() {
  return (
    <aside className="w-72 border-l border-studio-800 bg-studio-950 text-slate-200 p-3 overflow-y-auto">
      <div className="mb-4">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">RENDER QUEUE</h3>
        <div className="space-y-2 text-sm">
          <div className="flex items-center justify-between bg-studio-800/50 p-2 rounded">
            <span>▶ out-v1.mp4 · high · local</span>
            <span className="text-accent">███░░░ 38%</span>
          </div>
          <div className="flex items-center justify-between text-slate-400 p-2">
            <span>○ out-draft.mp4 · draft · local</span>
            <span>queued</span>
          </div>
        </div>
      </div>

      <div>
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">INSPECTOR</h3>
        <div className="text-sm space-y-1 bg-studio-800/50 p-2 rounded">
          <div className="font-medium">▸ Project</div>
          <div className="pl-2 text-slate-300">black-holes-explainer</div>
          <div className="pl-2 text-xs text-slate-400">30s · 1280×720 · 30fps</div>
        </div>
      </div>
    </aside>
  );
}

// --- StatusStrip ---
export function StatusStrip() {
  return (
    <footer className="bg-studio-950 border-t border-studio-800 px-4 py-1 text-xs flex items-center gap-4 text-slate-300">
      <span>● sd-server idle</span>
      <span>● render worker ready</span>
      <span className="ml-auto">↑ 3 tools run</span>
      <span className="w-32 bg-studio-800 h-2 rounded overflow-hidden">
        <div className="bg-accent h-full w-[40%]" />
      </span>
      <span>████░░ 40% render</span>
    </footer>
  );
}

// --- TabBar ---
export function TabBar() {
  const tabs = ["Chat", "Timeline", "Assets", "Renders"];
  return (
    <div className="flex border-b border-studio-800 bg-studio-950 text-sm">
      {tabs.map((tab, i) => (
        <button
          key={tab}
          className={`px-4 py-2 border-r border-studio-800 ${i === 0 ? "bg-studio-800 text-accent" : "text-slate-300 hover:bg-studio-800/50"}`}
        >
          {tab}
        </button>
      ))}
    </div>
  );
}
