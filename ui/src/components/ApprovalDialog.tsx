/**
 * Approval Dialog Component (Phase 11).
 *
 * Tauri dialog for ApprovalRequest (Allow / Deny / Edit), with an "always allow" checkbox.
 */

import React, { useState } from "react";

interface ApprovalDialogProps {
  request: {
    id: string;
    kind: string;
    payload: any;
  };
  onAnswer: (answer: "allow" | "deny" | "edit", alwaysAllow: boolean) => void;
}

export function ApprovalDialog({ request, onAnswer }: ApprovalDialogProps) {
  const [alwaysAllow, setAlwaysAllow] = useState(false);

  // Mock payload display based on kind
  const isBash = request.kind === "bash";
  const command = isBash ? (request.payload as any)?.command || "$ npx hyperframes render --quality high" : "Tool execution";

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-studio-900 border border-studio-700 rounded-lg p-4 max-w-md w-full shadow-xl">
        <h3 className="text-sm font-semibold text-slate-200 mb-3">Permission Request</h3>
        
        <div className="bg-studio-800/50 border border-studio-700 rounded p-3 mb-4 font-mono text-xs">
          <div className="text-slate-300">{command}</div>
        </div>

        <div className="flex items-center gap-2 mb-4">
          <input
            type="checkbox"
            id="alwaysAllow"
            checked={alwaysAllow}
            onChange={(e) => setAlwaysAllow(e.target.checked)}
            className="w-4 h-4 bg-studio-800 border-studio-700 rounded text-accent focus:ring-accent"
          />
          <label htmlFor="alwaysAllow" className="text-sm text-slate-300">
            Always allow this {request.kind} command
          </label>
        </div>

        <div className="flex gap-2 justify-end">
          <button
            onClick={() => onAnswer("deny", false)}
            className="px-3 py-1 bg-studio-700 hover:bg-studio-600 rounded text-sm text-slate-300"
          >
            Deny
          </button>
          <button
            onClick={() => onAnswer("edit", false)}
            className="px-3 py-1 bg-studio-700 hover:bg-studio-600 rounded text-sm text-slate-300"
          >
            Edit
          </button>
          <button
            onClick={() => onAnswer("allow", alwaysAllow)}
            className="px-3 py-1 bg-accent hover:bg-accent/80 rounded text-sm text-white font-medium"
          >
            Allow
          </button>
        </div>
      </div>
    </div>
  );
}
