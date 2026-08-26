/**
 * Timeline Tab (Phase 10).
 *
 * The composition's live preview (PreviewCanvas) above a scrubable track view
 * (TrackView). On entering, it parses the current composition and starts the
 * preview server; the playhead scrubs the tracks and drives the snapshot
 * button, which pins a frame at the current timecode.
 */

import { useEffect, useRef, useState } from "react";
import { useStore } from "../lib/store";
import { Commands } from "../lib/invoke";
import { PreviewCanvas } from "./PreviewCanvas";
import { TrackView } from "./TrackView";

export function TimelineTab() {
  const session = useStore((s) => s.session);
  const preview = useStore((s) => s.preview);
  const timeline = useStore((s) => s.timeline);
  const selectedClipId = useStore((s) => s.selectedClipId);
  const loadTimeline = useStore((s) => s.loadTimeline);
  const startPreview = useStore((s) => s.startPreview);
  const stopPreview = useStore((s) => s.stopPreview);
  const selectClip = useStore((s) => s.selectClip);

  const [playheadMs, setPlayheadMs] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [pinnedAt, setPinnedAt] = useState<number | null>(null);
  const timer = useRef<number | null>(null);

  const projectId = session?.current_project_id ?? null;
  const compositionId = session?.current_composition_id ?? "main";
  const key = `${projectId ?? ""}:${compositionId}`;
  const durationMs = timeline?.duration_ms ?? 0;

  // (Re)load the timeline + preview whenever the project or composition changes.
  useEffect(() => {
    if (!projectId) return;
    setPlaying(false);
    setPlayheadMs(0);
    void loadTimeline(projectId, compositionId);
    void startPreview();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, projectId]);

  // Tear down the preview server when leaving the tab.
  useEffect(() => {
    return () => {
      void stopPreview();
    };
  }, [stopPreview]);

  // Playhead clock while playing.
  useEffect(() => {
    if (!playing) return;
    timer.current = window.setInterval(() => {
      setPlayheadMs((t) => {
        if (durationMs <= 0) return t;
        return t >= durationMs ? 0 : t + 250;
      });
    }, 250);
    return () => {
      if (timer.current) clearInterval(timer.current);
    };
  }, [playing, durationMs]);

  if (!projectId) {
    return <EmptyState label="Open a project to see its timeline." />;
  }
  if (!timeline) {
    return <EmptyState label="No composition found in this project." />;
  }

  const aspect = timeline.width > 0 && timeline.height > 0 ? timeline.width / timeline.height : 16 / 9;
  const selectedClip = timeline.clips.find((c) => c.id === selectedClipId) ?? timeline.clips[0] ?? null;

  return (
    <div className="flex flex-1 flex-col overflow-hidden bg-canvas">
      <div className="flex min-h-0 flex-1 flex-col p-4">
        {/* Preview */}
        <div className="mb-3 flex min-h-0 flex-1 flex-col rounded-lg border border-line-soft bg-panel p-3">
          <PreviewCanvas aspect={aspect} onStart={() => void startPreview()} />
        </div>

        {/* Transport */}
        <Transport
          playheadMs={playheadMs}
          durationMs={durationMs}
          playing={playing}
          pinnedAt={pinnedAt}
          onTogglePlay={() => setPlaying((p) => !p)}
          onSeek={setPlayheadMs}
          onSnapshot={() => void pinSnapshot(projectId, playheadMs, setPinnedAt)}
        />

        {/* Tracks */}
        <div className="min-h-0 flex-1 rounded-lg border border-line-soft bg-panel">
          <TrackView
            tracks={timeline.tracks}
            clips={timeline.clips}
            durationMs={durationMs}
            playheadMs={playheadMs}
            selectedClipId={selectedClipId}
            onSeek={setPlayheadMs}
            onSelectClip={selectClip}
          />
        </div>
      </div>

      {selectedClip && (
        <div className="flex items-center gap-2 border-t border-line-soft px-4 py-1.5 text-xs text-ink-muted">
          <span className="text-[10px] uppercase tracking-wide text-ink-faint">Selected</span>
          <span className="mono text-ink">{selectedClip.id}</span>
          <span>·</span>
          <span>{fmtSec(selectedClip.start_s)}–{fmtSec(selectedClip.start_s + selectedClip.duration_s)}</span>
          {selectedClip.src && (
            <>
              <span>·</span>
              <span className="truncate" title={selectedClip.src}>
                {selectedClip.src.split(/[\\/]/).pop()}
              </span>
            </>
          )}
        </div>
      )}
    </div>
  );
}

async function pinSnapshot(
  projectId: string,
  ms: number,
  setPinnedAt: (ms: number | null) => void,
) {
  try {
    const res = await Commands.snapshot(projectId, ms);
    setPinnedAt(res.timecode_ms);
  } catch {
    /* snapshot failures surface via the error strip */
  }
}

function Transport({
  playheadMs,
  durationMs,
  playing,
  pinnedAt,
  onTogglePlay,
  onSeek,
  onSnapshot,
}: {
  playheadMs: number;
  durationMs: number;
  playing: boolean;
  pinnedAt: number | null;
  onTogglePlay: () => void;
  onSeek: (ms: number) => void;
  onSnapshot: () => void;
}) {
  const ratio = durationMs > 0 ? Math.min(1, playheadMs / durationMs) : 0;
  const justPinned = pinnedAt != null && Date.now() - pinnedAt < 1500;

  return (
    <div className="mb-3 flex items-center gap-3 rounded-lg border border-line-soft bg-panel px-3 py-2">
      <button
        className="icon-btn h-8 w-8"
        title={playing ? "Pause" : "Play"}
        aria-label={playing ? "Pause" : "Play"}
        onClick={onTogglePlay}
      >
        {playing ? "❚❚" : "▶"}
      </button>

      {/* Scrubber */}
      <div
        className="relative flex-1 h-6 cursor-crosshair rounded bg-fill-secondary"
        onClick={(e) => {
          const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
          onSeek(((e.clientX - rect.left) / rect.width) * durationMs);
        }}
        role="slider"
        aria-label="Playhead"
        aria-valuenow={Math.round(playheadMs)}
        aria-valumax={durationMs}
        aria-valuemin={0}
      >
        <div
          className="h-full bg-info/30"
          style={{ width: `${ratio * 100}%` }}
        />
        <div
          className="pointer-events-none absolute -top-0 bottom-0 w-0.5 bg-danger"
          style={{ left: `${ratio * 100}%` }}
        />
      </div>

      <span className="mono w-20 text-center text-xs text-ink-muted">{fmtSec(playheadMs / 1000)}</span>

      <button
        className="btn btn-sm"
        onClick={onSnapshot}
        title="Pin a frame at the current timecode"
      >
        {justPinned ? "✓ Pinned" : "📌 Snapshot"}
      </button>
    </div>
  );
}

function EmptyState({ label }: { label: string }) {
  return (
    <div className="flex h-full min-h-60 flex-col items-center justify-center gap-2 p-8 text-center">
      <div className="text-ink-faint text-lg">◳</div>
      <div className="text-xs text-ink-muted">{label}</div>
    </div>
  );
}

function fmtSec(sec: number): string {
  if (!Number.isFinite(sec) || sec <= 0) return "0:00";
  const total = Math.floor(sec);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}
