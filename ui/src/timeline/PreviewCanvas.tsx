/**
 * Preview Canvas (Timeline tab).
 *
 * Embeds the live HyperFrames preview server in an iframe, framed to the
 * composition's aspect ratio. Shows honest states: idle (start button),
 * starting (a quiet line, no spinner), running (the iframe), and error (the
 * cause + retry) — never a spinner that hides what is happening.
 */

import {useStore} from '../lib/store'

interface PreviewCanvasProps {
  /** Composition aspect ratio (width / height). Falls back to 16/9. */
  aspect: number
  /** Rendered by the parent when the user hits "start". */
  onStart: () => void | Promise<unknown>
}

export function PreviewCanvas({aspect, onStart}: PreviewCanvasProps) {
  const preview = useStore((s) => s.preview)
  const ratio = Number.isFinite(aspect) && aspect > 0 ? aspect : 16 / 9

  return (
    <div className="flex flex-1 items-center justify-center p-4">
      {/* Frame keeps the composition's aspect ratio and centers the canvas. */}
      <div
        className="relative flex h-full w-full items-center justify-center"
        style={{aspectRatio: `${ratio} / 1`}}
      >
        {preview.status === 'running' && preview.url ? (
          <iframe
            title="HyperFrames preview"
            src={preview.url}
            className="h-full w-full rounded-md border border-line-soft bg-black"
            sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
          />
        ) : preview.status === 'error' ? (
          <ErrorState message={preview.error} onRetry={() => void onStart()} />
        ) : preview.status === 'starting' ? (
          <Placeholder glyph="◳" label="Starting preview…" />
        ) : (
          <IdleState onStart={() => void onStart()} />
        )}
      </div>
    </div>
  )
}

function IdleState({onStart}: {onStart: () => void}) {
  return (
    <div className="flex max-w-xs flex-col items-center gap-3 text-center">
      <div className="flex h-12 w-12 items-center justify-center rounded-full bg-subtle text-ink-faint">
        ◳
      </div>
      <div className="space-y-1">
        <div className="text-[13px] font-medium text-ink-strong">No preview yet</div>
        <div className="text-xs text-ink-muted">
          Start the live HyperFrames preview to see this composition play in real time.
        </div>
      </div>
      <button className="btn btn-sm" onClick={onStart}>
        Start preview
      </button>
    </div>
  )
}

function ErrorState({message, onRetry}: {message: string | null; onRetry: () => void}) {
  return (
    <div className="flex max-w-xs flex-col items-center gap-3 text-center">
      <div className="flex h-12 w-12 items-center justify-center rounded-full bg-danger-bg text-danger">
        ⚠
      </div>
      <div className="space-y-1">
        <div className="text-[13px] font-medium text-ink-strong">Preview couldn't start</div>
        <div className="text-xs text-ink-muted">{message ?? 'Preview failed to start.'}</div>
      </div>
      <button className="btn btn-sm" onClick={onRetry}>
        Try again
      </button>
    </div>
  )
}

function Placeholder({glyph, label}: {glyph: string; label: string}) {
  return (
    <div className="flex flex-col items-center gap-2 text-ink-faint">
      <span className="text-lg">{glyph}</span>
      <span className="text-xs">{label}</span>
    </div>
  )
}
