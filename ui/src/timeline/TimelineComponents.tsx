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
    <div className="border-b border-line-soft bg-canvas p-4">
      <div className="flex aspect-video items-center justify-center rounded-xl border border-line-soft bg-panel">
        <div className="text-[13px] text-ink-faint">[ ▶ live preview canvas 1280×720 ]</div>
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
    <div className="space-y-2 bg-canvas p-4">
      <h3 className="rail-label">Tracks</h3>
      {tracks.map((track) => (
        <div key={track.name} className="flex items-center gap-2 text-[13px]">
          <span className="mono w-16 text-xs text-ink-muted">{track.label}</span>
          <span className="relative h-6 flex-1 overflow-hidden rounded-md border border-line-soft bg-panel">
            <div className="absolute inset-y-0 left-0 w-[30%] bg-fill-secondary" />
            <span className="absolute inset-y-0 right-2 flex items-center truncate pr-2 text-xs text-ink-muted">
              {track.content || "—"}
            </span>
          </span>
        </div>
      ))}

      {/* Selected clip inspector preview */}
      <div className="mt-4 space-y-1.5 border-t border-line-soft pt-3 text-[13px] text-ink">
        <div className="flex items-center gap-2">
          <span className="mono text-xs text-ink-strong">▌title</span>
          <span>"What is a black hole?"</span>
        </div>
        <div className="flex items-center gap-2 text-xs text-ink-muted">
          <span className="numeric">t=07.2s</span>
          <span className="ml-auto flex gap-1.5">
            <button className="btn btn-ghost btn-sm h-6 px-2 text-xs">◀ ▶ ■</button>
            <button className="btn btn-ghost btn-sm h-6 px-2 text-xs">Edit in chat</button>
            <button className="btn btn-ghost btn-sm h-6 px-2 text-xs">Snapshot</button>
            <button className="btn btn-ghost btn-sm h-6 px-2 text-xs">Add keyframe</button>
          </span>
        </div>
      </div>
    </div>
  );
}

// --- TimelineTab ---
export function TimelineTab() {
  return (
    <div className="flex h-full flex-col bg-canvas text-ink">
      <PreviewCanvas />
      <TrackView />
    </div>
  );
}
