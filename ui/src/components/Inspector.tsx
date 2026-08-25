/**
 * Inspector Component (Phase 10).
 *
 * Contextual panels (project, selected clip, selected asset, generation source)
 * per design.md §6; every editable field's "edit in chat" pre-fills the Composer
 * with a targeted instruction.
 */

import React from "react";

interface InspectorProps {
  selectedType: "project" | "clip" | "asset" | "source";
  selectedData?: any;
}

export function Inspector({ selectedType, selectedData }: InspectorProps) {
  if (selectedType === "project") {
    return (
      <div>
        <h3 className="rail-label">Project</h3>
        <div className="space-y-1 rounded-lg border border-line-soft bg-canvas p-3 text-[13px] text-ink">
          <div className="font-medium text-ink-strong">black-holes-explainer</div>
          <div className="text-xs text-ink-muted">30s · 1280×720 · 30fps</div>
          <div className="text-xs text-ink-muted">
            harness <span className="text-ink">a-coder-cli</span>
          </div>
          <div className="text-xs text-ink-muted">
            model <span className="text-ink">navya/auto</span>
          </div>
          <div className="text-xs text-ink-muted">4 scenes · 6 assets</div>
        </div>
      </div>
    );
  }

  if (selectedType === "clip") {
    return (
      <div>
        <h3 className="rail-label">Selected: scene2 clip</h3>
        <div className="space-y-1 rounded-lg border border-line-soft bg-canvas p-3 text-[13px] text-ink">
          <div className="flex justify-between">
            <span className="text-ink-muted">start</span>
            <span className="numeric">06.0s ▾</span>
          </div>
          <div className="flex justify-between">
            <span className="text-ink-muted">dur</span>
            <span className="numeric">06.0s ▾</span>
          </div>
          <div className="flex justify-between">
            <span className="text-ink-muted">media</span>
            <span className="mono text-xs">scene2.png</span>
          </div>
          <div className="flex justify-between">
            <span className="text-ink-muted">motion</span>
            <span>reveal-up</span>
          </div>
          <div className="mt-2 border-t border-line-soft pt-2">
            <div className="mb-1 text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-faint">Variables</div>
            <div className="flex justify-between text-xs">
              <span className="text-ink-muted">title</span>
              <span>"The horizon"</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-ink-muted">palette</span>
              <span className="mono">#202020</span>
            </div>
          </div>
          <button className="btn btn-ghost btn-sm mt-2 w-full text-xs">Edit in chat</button>
        </div>
      </div>
    );
  }

  if (selectedType === "source") {
    return (
      <div>
        <h3 className="rail-label">Generation Source</h3>
        <div className="space-y-1.5 rounded-lg border border-line-soft bg-canvas p-3 text-[13px] text-ink">
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-ok">●</span>
            <span className="font-medium text-ink-strong">Cloud (Navya)</span>
          </div>
          <div className="flex items-center gap-2 text-ink-muted">
            <span className="text-[10px] text-ink-faint">○</span>
            <span>Local (sd-server)</span>
          </div>
        </div>
      </div>
    );
  }

  return null;
}
