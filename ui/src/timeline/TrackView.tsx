/**
 * Track View (Timeline tab).
 *
 * Renders the parsed composition as stacked tracks: one row per
 * `data-track-index`, each clip positioned by its absolute start time so the
 * playhead aligns with the preview. Clicking the lane scrubs the playhead;
 * clicking a clip selects it for the inspector.
 */

import type {Clip, TimelineTrack} from '../lib/invoke'

interface TrackViewProps {
  tracks: TimelineTrack[]
  clips: Clip[]
  durationMs: number
  playheadMs: number
  selectedClipId: string | null
  onSeek: (ms: number) => void
  onSelectClip: (clipId: string | null) => void
}

export function TrackView({
  tracks,
  clips,
  durationMs,
  playheadMs,
  selectedClipId,
  onSeek,
  onSelectClip,
}: TrackViewProps) {
  const duration = durationMs > 0 ? durationMs : 1

  return (
    <div className="relative w-full px-4 py-3">
      {/* Time ruler */}
      <div className="relative mb-1 flex h-5 items-center justify-between text-[10px] text-ink-faint">
        {rTicks(durationMs).map((t) => (
          <span key={t.label} className="mono" style={{left: `${(t.ms / duration) * 100}%`}}>
            {t.label}
          </span>
        ))}
      </div>

      {tracks.length === 0 ? (
        <div className="text-xs text-ink-faint">No clips — the composition has no tracks yet.</div>
      ) : (
        <div className="relative space-y-1">
          {tracks.map((track) => {
            const trackClips = clips.filter((c) => c.track_index === track.index)
            return (
              <div key={track.index} className="flex items-center gap-3">
                <span className="w-24 shrink-0 truncate text-xs text-ink-muted" title={track.name}>
                  {track.name}
                </span>
                <div
                  className="relative flex-1 cursor-crosshair rounded bg-fill-secondary"
                  style={{height: 34}}
                  onClick={(e) => void scrub(e, track.index, duration, onSeek)}
                  role="slider"
                  aria-label={`Track ${track.index}`}
                  aria-valuenow={playheadMs}
                  aria-valumax={durationMs}
                  aria-valuemin={0}
                >
                  {trackClips.map((clip) => {
                    const left = ((clip.start_s * 1000) / duration) * 100
                    const width = Math.max((clip.duration_s * 1000) / duration, 0.6)
                    const selected = selectedClipId === clip.id
                    return (
                      <button
                        key={clip.id}
                        type="button"
                        className="absolute top-1 bottom-1 flex items-center overflow-hidden rounded border border-line-selected px-1.5 text-left text-xs text-ink-strong transition-colors hover:bg-subtle"
                        style={{
                          left: `${left}%`,
                          width: `${width}%`,
                          fontFamily: 'var(--mono)',
                          background: selected ? 'var(--color-selected)' : 'var(--color-subtle)',
                        }}
                        onClick={(e) => {
                          e.stopPropagation()
                          onSelectClip(clip.id)
                        }}
                        title={clip.id}
                      >
                        <span className="truncate">{shortName(clip)}</span>
                      </button>
                    )
                  })}
                </div>
              </div>
            )
          })}

          {/* Playhead spans the lanes + label */}
          <div className="relative h-0 w-full" aria-hidden>
            <div
              className="pointer-events-none absolute top-0 bottom-0 w-px bg-danger transition-[left] duration-75 ease-out"
              style={{left: `calc(12px + ${(playheadMs / duration) * 100}%)`}}
            />
          </div>
        </div>
      )}
    </div>
  )
}

function shortName(clip: Clip): string {
  if (clip.src) {
    const parts = clip.src.split(/[\\/]/)
    return parts[parts.length - 1] ?? clip.src
  }
  return clip.id
}

/** Map a click's x-offset within the lane to a composition timecode. */
function scrub(
  e: React.MouseEvent,
  _trackIndex: number,
  duration: number,
  onSeek: (ms: number) => void,
) {
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width))
  onSeek(ratio * duration)
}

/** Even time ticks across the composition for the ruler. */
function rTicks(durationMs: number): {ms: number; label: string}[] {
  const totalSec = Math.ceil(durationMs / 1000)
  const step = pickStep(totalSec)
  const ticks: {ms: number; label: string}[] = []
  for (let s = 0; s <= totalSec; s += step) {
    ticks.push({ms: s * 1000, label: formatTime(s * 1000)})
  }
  return ticks
}

function pickStep(totalSec: number): number {
  for (const s of [1, 2, 5, 10, 15, 30, 60]) {
    if (totalSec <= s * 8) return s
  }
  return 60
}

function formatTime(ms: number): string {
  const totalSec = Math.round(ms / 1000)
  const m = Math.floor(totalSec / 60)
  const s = totalSec % 60
  return `${m}:${String(s).padStart(2, '0')}`
}
