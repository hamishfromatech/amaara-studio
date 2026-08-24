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
      <div className="space-y-2 text-sm">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">PROJECT</h3>
        <div className="bg-studio-800/50 p-2 rounded space-y-1">
          <div className="font-medium">black-holes-explainer</div>
          <div className="text-xs text-slate-400">30s · 1280×720 · 30fps</div>
          <div className="text-xs text-slate-400">harness a-coder-cli</div>
          <div className="text-xs text-slate-400">model navya/auto</div>
          <div className="text-xs text-slate-400">4 scenes · 6 assets</div>
        </div>
      </div>
    );
  }

  if (selectedType === "clip") {
    return (
      <div className="space-y-2 text-sm">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">SELECTED: scene2 clip</h3>
        <div className="bg-studio-800/50 p-2 rounded space-y-1">
          <div className="flex justify-between"><span>start</span><span>06.0s ▾</span></div>
          <div className="flex justify-between"><span>dur</span><span>06.0s ▾</span></div>
          <div className="flex justify-between"><span>media</span><span>scene2.png</span></div>
          <div className="flex justify-between"><span>motion</span><span>reveal-up</span></div>
          <div className="mt-2 pt-2 border-t border-studio-700">
            <div className="text-xs font-semibold mb-1">─ variables ──</div>
            <div className="flex justify-between text-xs"><span>title</span><span>"The horizon"</span></div>
            <div className="flex justify-between text-xs"><span>palette</span><span>#0a1a2f</span></div>
          </div>
          <button className="w-full mt-2 px-2 py-1 bg-studio-700 hover:bg-studio-600 rounded text-xs text-accent">
            [edit in chat]
          </button>
        </div>
      </div>
    );
  }

  if (selectedType === "source") {
    return (
      <div className="space-y-2 text-sm">
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">Generation source</h3>
        <div className="bg-studio-800/50 p-2 rounded space-y-1">
          <div className="flex items-center gap-2"><span className="text-accent">●</span><span>Cloud (Navya)</span></div>
          <div className="flex items-center gap-2 text-slate-400"><span>○</span><span>Local (sd-server)</span></div>
        </div>
      </div>
    );
  }

  return null;
}
