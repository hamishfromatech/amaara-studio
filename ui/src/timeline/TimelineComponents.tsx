/**
 * Timeline Tab Components (Phase 10).
 *
 * The assembled HyperFrames composition as a preview + track view, synced to
 * the chat. When the agent edits composition.html, this updates live.
 */

import React from "react";

// --- PreviewCanvas ---
export function PreviewCanvas() {
  return (
    <div className="bg-studio-950 border-b border-studio-800 p-4">
      <div className="aspect-video bg-studio-800 rounded-lg flex items-center justify-center border border-studio-700">
        <div className="text-slate-400 text-sm">
          [ ▶ live preview canvas 1280×720 ]
        </div>
      </div>
    </div>
  );
}

// --- TrackView ---
export function TrackView() {
  const tracks = [
    { name: "title", label: "title", content: '"What is a black hole?"', start: "07.2s" },
    { name: "scene1", label: "scene1", content: "", start: "00.0s" },
    { name: "scene2", label: "scene2", content: "", start: "05.0s" },
    { name: "vo", label: "vo", content: '"...a region where..."', start: "07.2s" },
    { name: "bgm", label: "bgm", content: "", start: "00.0s" },
  ];

  return (
    <div className="p-4 space-y-2 bg-studio-950">
      <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">tracks</h3>
      {tracks.map((track, i) => (
        <div key={track.name} className="flex items-center gap-2 text-sm">
          <span className="w-16 text-slate-400 font-mono">{track.label}</span>
          <span className="flex-1 bg-studio-800 h-6 rounded relative overflow-hidden">
            <div className="absolute inset-y-0 left-0 w-[30%] bg-accent/30" />
            <span className="absolute inset-y-0 right-2 flex items-center text-xs text-slate-300 truncate pr-2">
              {track.content || "—"}
            </span>
          </span>
        </div>
      ))}

      {/* Selected clip inspector preview */}
      <div className="mt-4 pt-4 border-t border-studio-800 text-sm space-y-1">
        <div className="flex items-center gap-2">
          <span className="font-mono text-slate-300">▌title</span>
          <span>"What is a black hole?"</span>
        </div>
        <div className="flex items-center gap-2 text-xs text-slate-400">
          <span>t=07.2s</span>
          <span className="ml-auto flex gap-2">
            <button className="hover:text-accent">◀ ▶ ■</button>
            <button className="text-accent hover:opacity-80">[edit in chat]</button>
            <button className="hover:text-accent">[snapshot]</button>
            <button className="hover:text-accent">[add keyframe]</button>
          </span>
        </div>
      </div>
    </div>
  );
}

// --- TimelineTab ---
export function TimelineTab() {
  return (
    <div className="flex flex-col h-full bg-studio-900 text-slate-100">
      <PreviewCanvas />
      <TrackView />
    </div>
  );
}
