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
    <div className="scrim">
      <div className="modal w-full max-w-md p-5">
        <h3 className="mb-3 text-sm font-semibold text-ink-strong">Permission Request</h3>

        <div className="mono mb-4 rounded-md border border-line-soft bg-panel p-3 text-xs leading-relaxed text-ink">
          {command}
        </div>

        <div className="mb-5 flex items-center gap-2">
          <input
            type="checkbox"
            id="alwaysAllow"
            checked={alwaysAllow}
            onChange={(e) => setAlwaysAllow(e.target.checked)}
            className="h-4 w-4 cursor-pointer"
          />
          <label htmlFor="alwaysAllow" className="cursor-pointer text-[13px] text-ink">
            Always allow this {request.kind} command
          </label>
        </div>

        <div className="flex justify-end gap-2">
          <button onClick={() => onAnswer("deny", false)} className="btn btn-ghost btn-sm">
            Deny
          </button>
          <button onClick={() => onAnswer("edit", false)} className="btn btn-sm">
            Edit
          </button>
          <button onClick={() => onAnswer("allow", alwaysAllow)} className="btn btn-primary btn-sm">
            Allow
          </button>
        </div>
      </div>
    </div>
  );
}
