/**
 * Approval Dialog (Phase 11, design.md §9 — "harness asks, studio answers
 * natively").
 *
 * Shown when a harness raises an ApprovalRequest. Behaves like a native
 * modal: focus moves into the dialog on open (initial focus on the safe
 * default), Tab is trapped inside, Esc = Deny, Enter = Allow, and focus
 * returns to the trigger on close.
 *
 * Answers:
 *   Allow  → approved (optionally persisting a scoped "always allow" rule)
 *   Deny   → cancelled
 *   Edit   → the request's command/value is editable; the edited text is
 *            relayed back (a-coder-cli input/editor dialogs; boolean-only
 *            harness protocols ignore the value and treat it as allow).
 */

import React, {useEffect, useRef, useState} from 'react'

interface ApprovalDialogProps {
  request: {
    id: string
    kind: string
    payload: unknown
  }
  onAnswer: (answer: 'allow' | 'deny' | 'edit', alwaysAllow: boolean, value?: string) => void
}

/** Best-effort extraction of the human-facing command from the payload. */
function extractCommand(request: ApprovalDialogProps['request']): string {
  const p = request.payload as Record<string, unknown> | null
  if (p && typeof p === 'object') {
    for (const key of ['command', 'cmd', 'value', 'text', 'prompt', 'path']) {
      const v = p[key]
      if (typeof v === 'string' && v.trim()) return v
    }
  }
  if (typeof request.payload === 'string' && request.payload.trim()) {
    return request.payload
  }
  return request.kind === 'bash' ? '$ (no command in payload)' : 'Tool execution'
}

function payloadJson(request: ApprovalDialogProps['request']): string | null {
  try {
    const s = JSON.stringify(request.payload, null, 2)
    return s && s !== 'null' ? s : null
  } catch {
    return null
  }
}

/** Collect the focusable elements inside a root (focus-trap helper). */
function focusable(root: HTMLElement): HTMLElement[] {
  return Array.from(
    root.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
    ),
  ).filter((el) => !el.hasAttribute('disabled'))
}

export function ApprovalDialog({request, onAnswer}: ApprovalDialogProps) {
  const [alwaysAllow, setAlwaysAllow] = useState(false)
  const [editing, setEditing] = useState(false)
  const [editValue, setEditValue] = useState('')
  const dialogRef = useRef<HTMLDivElement | null>(null)
  const primaryRef = useRef<HTMLButtonElement | null>(null)
  const editRef = useRef<HTMLTextAreaElement | null>(null)
  const restoreRef = useRef<HTMLElement | null>(null)

  const command = extractCommand(request)

  // Focus management (native-modal semantics): remember the trigger, move
  // focus to the primary action on open, restore on unmount.
  useEffect(() => {
    restoreRef.current = document.activeElement as HTMLElement | null
    const t = window.setTimeout(() => {
      ;(editing ? editRef.current : primaryRef.current)?.focus()
    }, 0)
    return () => {
      window.clearTimeout(t)
      restoreRef.current?.focus?.()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Enter focuses the edit box when entering edit mode.
  useEffect(() => {
    if (editing) editRef.current?.focus()
    else primaryRef.current?.focus()
  }, [editing])

  // Keyboard: Esc = deny (safe default), Enter = allow / submit edit, Tab
  // trapped inside the dialog.
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onAnswer('deny', false)
      return
    }
    if (e.key === 'Enter' && !e.shiftKey && !editing) {
      e.preventDefault()
      onAnswer('allow', alwaysAllow)
      return
    }
    if (e.key === 'Tab' && dialogRef.current) {
      const els = focusable(dialogRef.current)
      if (els.length === 0) return
      const first = els[0]
      const last = els[els.length - 1]
      const active = document.activeElement
      if (e.shiftKey && active === first) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && active === last) {
        e.preventDefault()
        first.focus()
      }
    }
  }

  const startEdit = () => {
    setEditValue(command)
    setEditing(true)
  }

  return (
    <div
      className="scrim"
      onMouseDown={(e) => e.target === e.currentTarget && onAnswer('deny', false)}
    >
      <div
        ref={dialogRef}
        className="modal w-full max-w-md p-5"
        role="dialog"
        aria-modal="true"
        aria-labelledby="approval-title"
        onKeyDown={onKeyDown}
      >
        <div className="mb-3 flex items-center justify-between">
          <h3 id="approval-title" className="text-sm font-semibold text-ink-strong">
            Permission Request
          </h3>
          <span
            className="chip"
            style={{cursor: 'default', height: 22, fontSize: 11}}
            title="Request kind"
          >
            {request.kind}
          </span>
        </div>

        <div className="mb-2 text-[13px] text-ink-muted">
          <span className="font-medium text-ink-strong">The agent</span> wants to:
        </div>

        {!editing ? (
          <div className="mono mb-4 max-h-40 overflow-y-auto rounded-md border border-line-soft bg-panel p-3 text-xs leading-relaxed text-ink">
            {command}
          </div>
        ) : (
          <div className="mb-4">
            <label
              htmlFor="approval-edit"
              className="mb-1 block text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-faint"
            >
              Edit before running
            </label>
            <textarea
              id="approval-edit"
              ref={editRef}
              className="input mono"
              rows={4}
              value={editValue}
              onChange={(e) => setEditValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                  e.preventDefault()
                  onAnswer('edit', alwaysAllow, editValue)
                }
              }}
            />
            <div className="mt-1 text-[11px] text-ink-faint">
              ⌘↵ sends the edited value back to the agent
            </div>
          </div>
        )}

        {payloadJson(request) && !editing && (
          <details className="mb-4 text-xs text-ink-muted">
            <summary className="cursor-pointer text-ink-soft">full payload</summary>
            <pre className="mono mt-2 max-h-32 overflow-y-auto rounded-md border border-line-soft bg-panel p-2 text-[11px] leading-relaxed">
              {payloadJson(request)}
            </pre>
          </details>
        )}

        {!editing && (
          <div className="mb-5 flex items-center gap-2">
            <input
              type="checkbox"
              id="alwaysAllow"
              checked={alwaysAllow}
              onChange={(e) => setAlwaysAllow(e.target.checked)}
              className="h-4 w-4 cursor-pointer"
            />
            <label htmlFor="alwaysAllow" className="cursor-pointer text-[13px] text-ink">
              Always allow this {request.kind} request
            </label>
          </div>
        )}

        <div className="flex justify-end gap-2">
          {editing ? (
            <>
              <button onClick={() => setEditing(false)} className="btn btn-ghost btn-sm">
                Cancel
              </button>
              <button
                ref={primaryRef}
                onClick={() => onAnswer('edit', alwaysAllow, editValue)}
                className="btn btn-primary btn-sm"
              >
                Send edit
              </button>
            </>
          ) : (
            <>
              <button onClick={() => onAnswer('deny', false)} className="btn btn-ghost btn-sm">
                Deny
              </button>
              <button onClick={startEdit} className="btn btn-sm">
                Edit
              </button>
              <button
                ref={primaryRef}
                onClick={() => onAnswer('allow', alwaysAllow)}
                className="btn btn-primary btn-sm"
              >
                Allow
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  )
}
