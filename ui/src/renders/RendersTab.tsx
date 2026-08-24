/**
 * Renders Tab Component (Phase 7).
 *
 * The queue with rich progress, not a blocking dialog. Shows target, quality,
 * live bar with frame count + ETA, and [open]/[reveal]/[retry]/[logs] controls.
 */

import React from "react";

export function RendersTab() {
  return (
    <div className="flex-1 overflow-y-auto p-4 bg-studio-900 text-slate-100">
      <h2 className="text-lg font-bold mb-4">Renders</h2>

      {/* Render queue */}
      <div className="space-y-2 mb-6">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400">RENDER QUEUE</h3>
        <div className="space-y-1">
          {/* Running job */}
          <div className="flex items-center justify-between bg-studio-800/50 p-2 rounded border border-studio-700">
            <div className="flex items-center gap-2">
              <span className="text-accent">▶</span>
              <span className="text-sm">out-v1.mp4 · high · local</span>
            </div>
            <div className="flex items-center gap-2 text-xs">
              <span className="bg-studio-700 px-2 py-1 rounded">███░░░ 38%</span>
              <span>frame 432/1140</span>
              <button className="px-2 py-1 bg-studio-700 hover:bg-studio-600 rounded text-xs">⏸</button>
              <button className="px-2 py-1 bg-studio-700 hover:bg-studio-600 rounded text-xs">✕</button>
            </div>
          </div>

          {/* Queued job */}
          <div className="flex items-center justify-between text-slate-400 p-2">
            <span>○ out-draft.mp4 · draft · local</span>
            <span>queued</span>
          </div>

          {/* Completed job */}
          <div className="flex items-center justify-between bg-studio-800/30 p-2 rounded border border-studio-700">
            <div className="flex items-center gap-2">
              <span className="text-green-400">✓</span>
              <span className="text-sm">out-v0.mp4 · high · local</span>
            </div>
            <div className="flex gap-2 text-xs">
              <button className="px-2 py-1 bg-studio-700 hover:bg-studio-600 rounded">[open]</button>
              <button className="px-2 py-1 bg-studio-700 hover:bg-studio-600 rounded">[reveal]</button>
            </div>
          </div>

          {/* Failed job */}
          <div className="flex items-center justify-between bg-red-900/30 p-2 rounded border border-red-800">
            <div className="flex items-center gap-2">
              <span className="text-red-400">✗</span>
              <span className="text-sm">out-480p.mp4 · high · cloud — Chrome timeout</span>
            </div>
            <div className="flex gap-2 text-xs">
              <button className="px-2 py-1 bg-red-700 hover:bg-red-600 rounded">[retry]</button>
              <button className="px-2 py-1 bg-red-700 hover:bg-red-600 rounded">[logs]</button>
            </div>
          </div>
        </div>
      </div>

      {/* Selected job detail */}
      <div className="border-t border-studio-800 pt-4">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">SELECTED: out-v1.mp4</h3>
        <div className="grid grid-cols-2 gap-2 text-sm mb-4">
          <div>target: local ▾</div>
          <div>quality: high ▾</div>
          <div>codec: h264 ▾</div>
          <div>size: 1280×720 ▾</div>
          <div>fps: 30 ▾</div>
          <div>frames: 1140</div>
        </div>

        {/* Progress bar */}
        <div className="mb-2">
          <div className="flex justify-between text-xs mb-1">
            <span>progress</span>
            <span>38% · 432/1140</span>
          </div>
          <div className="w-full bg-studio-700 h-3 rounded overflow-hidden">
            <div className="bg-accent h-full w-[38%]" />
          </div>
        </div>

        <div className="text-xs text-slate-400 mb-2">stage: rendering frames (Chrome headless)</div>

        {/* Last frame preview */}
        <div className="bg-studio-800/50 border border-studio-700 rounded p-4 text-center text-sm text-slate-400 mb-2">
          [ last rendered frame preview ]
        </div>

        {/* Controls */}
        <div className="flex gap-2 text-xs">
          <button className="px-3 py-1 bg-studio-700 hover:bg-studio-600 rounded">[⏓ Reveal file when done]</button>
          <button className="px-3 py-1 bg-studio-700 hover:bg-studio-600 rounded">[⏸ Pause]</button>
          <button className="px-3 py-1 bg-studio-700 hover:bg-studio-600 rounded">[✕ Cancel]</button>
        </div>
      </div>
    </div>
  );
}
