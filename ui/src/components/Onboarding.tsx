/**
 * Onboarding & Keyboard Map (Phase 16).
 *
 * Onboarding (first launch: connect Navya / start local / pick harness) and
 * new-project (agent runs the intent interview in-chat, no wizard) — design.md §8.
 * Keyboard map (design.md §10): ⌘K palette, ⌘⇧Enter steer, ⌘. abort, ⌘1..4 tabs,
 * ⌘R render, ⌘\ / ⌘/ rails, Space/JKL timeline, ⌘E edit-in-chat, ? shortcuts overlay.
 */

import React from "react";

export function OnboardingScreen() {
  return (
    <div className="flex flex-col items-center justify-center h-full bg-studio-900 text-slate-100 p-8">
      <h1 className="text-3xl font-bold mb-4">Navya Studio</h1>
      <p className="text-center text-slate-300 mb-8 max-w-md">
        A content-creation studio powered by AI. Direct an agent. Watch it make. Render to video.
      </p>

      <div className="grid grid-cols-2 gap-6 max-w-2xl w-full">
        <button className="bg-studio-800 hover:bg-studio-700 p-4 rounded-lg text-left transition-colors">
          <h3 className="font-semibold mb-1">Connect Navya Cloud</h3>
          <p className="text-sm text-slate-400">Paste your API key →</p>
        </button>
        <button className="bg-studio-800 hover:bg-studio-700 p-4 rounded-lg text-left transition-colors">
          <h3 className="font-semibold mb-1">Start with a local model</h3>
          <p className="text-sm text-slate-400">(sd.cpp + LLM)</p>
        </button>
      </div>

      <div className="mt-8 text-center text-sm text-slate-400">
        …or open an existing project
      </div>

      <div className="mt-6 flex items-center gap-2 text-sm">
        <span>Pick a harness first:</span>
        <span className="font-mono bg-studio-800 px-2 py-1 rounded">● a-coder-cli</span>
        <span className="text-slate-400">○ Claude Code ○ Codex ○ Hermes …</span>
      </div>
    </div>
  );
}

export const KeyboardShortcuts = {
  cmdK: "Command Palette (jump to project, file, action, model)",
  cmdN: "New project",
  cmdEnter: "Send chat (normal)",
  cmdShiftEnter: "Send as steer (interrupt mid-turn)",
  cmdAltEnter: "Send as follow-up (queue after turn)",
  cmdPeriod: "Stop / abort current agent turn",
  cmdR: "Render current composition (last quality)",
  cmdShiftR: "Render… (choose quality + target)",
  cmd1to4: "Center tabs: chat / timeline / assets / renders",
  cmdBackslash: "Toggle left rail",
  cmdSlash: "Toggle right rail",
  cmdComma: "Settings",
  space: "Play/pause timeline preview at playhead",
  jkl: "Timeline shuttle (← play →)",
  cmdE: "'edit in chat' the selected clip/asset",
  questionMark: "Show shortcuts overlay",
};
