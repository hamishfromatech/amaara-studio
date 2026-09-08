/**
 * Inspector — clip detail panel (Phase 10).
 *
 * Shows the currently selected clip's timing, media source, and composition
 * variables, with an "edit in chat" button that pre-fills a targeted
 * instruction for the agent. Renders `null` when no clip is selected so the
 * parent can fall back to the project summary.
 */

import type {Clip} from '../lib/invoke'

interface InspectorProps {
  clip: Clip | null
  /** Called with a targeted instruction when the user hits "edit in chat". */
  onEdit: (instruction: string) => void
}

export function Inspector({clip, onEdit}: InspectorProps) {
  if (!clip) return null
  const vars = parseVariables(clip.variables)
  const start = clip.start_s
  const end = clip.start_s + clip.duration_s

  return (
    <div>
      <h3 className="rail-label">Selected clip</h3>
      <div className="text-[13px] font-medium text-ink-strong">{clip.id}</div>
      <div className="mt-1 space-y-1 rounded-lg border border-line-soft bg-canvas p-2 text-xs text-ink-muted">
        <div className="flex justify-between">
          <span>start</span>
          <span className="text-ink">{fmtSec(start)} ▾</span>
        </div>
        <div className="flex justify-between">
          <span>dur</span>
          <span className="text-ink">{fmtSec(clip.duration_s)} ▾</span>
        </div>
        {clip.src && (
          <div className="flex justify-between">
            <span>media</span>
            <span className="mono truncate text-left text-xs text-ink" title={clip.src}>
              {clip.src.split(/[\/]/).pop()}
            </span>
          </div>
        )}
      </div>
      {Object.keys(vars).length > 0 && (
        <div className="mt-2 rounded-lg border border-line-soft bg-canvas p-2">
          <div className="mb-1 text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-faint">
            Variables
          </div>
          {Object.entries(vars).map(([k, v]) => (
            <div className="flex justify-between text-xs" key={k}>
              <span className="text-ink-muted">{k}</span>
              <span className="text-ink">{String(v)}</span>
            </div>
          ))}
        </div>
      )}
      <button
        className="btn btn-ghost btn-sm mt-2 w-full text-xs"
        title="Send a targeted edit instruction to the agent"
        onClick={() =>
          onEdit(`Adjust clip "${clip.id}" (currently ${fmtSec(start)}–${fmtSec(end)}): `)
        }
      >
        Edit in chat
      </button>
    </div>
  )
}

/** Parse the raw `data-composition-variables` JSON into a plain object. */
function parseVariables(raw: string | null): Record<string, unknown> {
  if (!raw) return {}
  try {
    const parsed = JSON.parse(raw)
    return parsed && typeof parsed === 'object' ? parsed : {}
  } catch {
    return {}
  }
}

function fmtSec(sec: number): string {
  if (!Number.isFinite(sec) || sec < 0) return '0:00'
  const total = Math.floor(sec)
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${m}:${String(s).padStart(2, '0')}`
}
