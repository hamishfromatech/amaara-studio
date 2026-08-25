/**
 * Renders Tab Component (Phase 7) — state-driven.
 *
 * The queue with live progress from studio://event: target, quality, frame
 * bar + stage, and [reveal]/[cancel] controls. Empty state when no renders.
 */

import { useEffect, useState } from "react";
import { useStore } from "../lib/store";
import { Commands, type RenderJob } from "../lib/invoke";

export function RendersTab() {
  const renders = useStore((s) => s.renders);
  const refreshRenders = useStore((s) => s.refreshRenders);
  const cancelRender = useStore((s) => s.cancelRender);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  useEffect(() => {
    void refreshRenders();
  }, [refreshRenders]);

  const selected = renders.find((r) => r.job_id === selectedId) ?? renders.find((r) => r.status === "running") ?? renders[0] ?? null;

  return (
    <div className="flex-1 overflow-y-auto bg-canvas p-5 text-ink">
      <h2 className="mb-4 text-base font-semibold text-ink-strong">Renders</h2>

      {renders.length === 0 ? (
        <div className="py-10 text-center text-[13px] text-ink-faint">
          No renders yet. Use the Render button (or the agent's render_to_video tool) to queue one.
        </div>
      ) : (
        <>
          {/* Render queue */}
          <div className="mb-6">
            <h3 className="rail-label">Render Queue</h3>
            <div className="space-y-1.5">
              {renders.map((job) => (
                <QueueRow
                  key={job.job_id}
                  job={job}
                  selected={selected?.job_id === job.job_id}
                  onSelect={() => setSelectedId(job.job_id)}
                  onCancel={() => void cancelRender(job.job_id)}
                />
              ))}
            </div>
          </div>

          {/* Selected job detail */}
          {selected && <JobDetail job={selected} onCancel={() => void cancelRender(selected.job_id)} />}
        </>
      )}
    </div>
  );
}

function statusIcon(job: RenderJob) {
  switch (job.status) {
    case "running":
      return <span className="text-info">▶</span>;
    case "done":
      return <span className="text-ok">✓</span>;
    case "failed":
      return <span className="text-danger">✗</span>;
    case "cancelled":
      return <span className="text-ink-faint">⊘</span>;
    default:
      return <span className="text-ink-soft">○</span>;
  }
}

function QueueRow({
  job,
  selected,
  onSelect,
  onCancel,
}: {
  job: RenderJob;
  selected: boolean;
  onSelect: () => void;
  onCancel: () => void;
}) {
  const label = `${job.composition_id || job.job_id} · ${job.quality} · ${job.target || "local"}`;
  const pct =
    job.progress && job.progress.total_frames
      ? Math.min(100, Math.round((job.progress.frame / job.progress.total_frames) * 100))
      : null;

  return (
    <div
      onClick={onSelect}
      className={`flex cursor-pointer items-center justify-between gap-2 rounded-lg border px-2.5 py-2 transition-colors ${
        selected
          ? "border-line-selected bg-panel"
          : "border-line-soft bg-panel hover:border-line"
      }`}
    >
      <div className="flex min-w-0 items-center gap-2">
        <span className="text-xs">{statusIcon(job)}</span>
        <span className="truncate text-[13px]">{label}</span>
        {job.status === "failed" && job.error && (
          <span className="truncate text-xs text-danger">— {job.error}</span>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-2 text-xs">
        {job.status === "running" && (
          <>
            <span className="mono rounded bg-subtle px-1.5 py-0.5 text-ink-strong">
              {pct != null ? `${pct}%` : "…"}
            </span>
            {job.progress?.total_frames != null && (
              <span className="numeric text-ink-muted">
                frame {job.progress.frame}/{job.progress.total_frames}
              </span>
            )}
            <button
              onClick={(e) => {
                e.stopPropagation();
                onCancel();
              }}
              className="btn btn-sm h-6 px-2 text-xs"
              title="Cancel"
            >
              ✕
            </button>
          </>
        )}
        {job.status === "queued" && <span className="text-ink-faint">queued</span>}
        {job.status === "done" && job.output_path && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              void Commands.revealInFolder(job.output_path!);
            }}
            className="btn btn-ghost btn-sm h-6 px-2 text-xs"
          >
            Reveal
          </button>
        )}
      </div>
    </div>
  );
}

function JobDetail({ job, onCancel }: { job: RenderJob; onCancel: () => void }) {
  const pct =
    job.progress && job.progress.total_frames
      ? Math.min(100, Math.round((job.progress.frame / job.progress.total_frames) * 100))
      : null;

  return (
    <div className="border-t border-line-soft pt-4">
      <h3 className="rail-label">Selected: {job.composition_id || job.job_id}</h3>
      <div className="mb-4 grid grid-cols-2 gap-1.5 text-[13px] text-ink">
        <div>
          <span className="text-ink-muted">target</span> {job.target || "local"}
        </div>
        <div>
          <span className="text-ink-muted">quality</span> {job.quality}
        </div>
        <div>
          <span className="text-ink-muted">status</span> {job.status}
        </div>
        <div className="numeric text-ink-muted">
          {job.started_at_ms ? new Date(job.started_at_ms).toLocaleTimeString() : "—"}
        </div>
      </div>

      {(job.status === "running" || job.status === "queued") && (
        <>
          <div className="mb-3">
            <div className="mb-1 flex justify-between text-xs text-ink-muted">
              <span>Progress</span>
              <span className="numeric">
                {pct != null
                  ? `${pct}% · ${job.progress?.frame}/${job.progress?.total_frames}`
                  : "starting…"}
              </span>
            </div>
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-fill-secondary">
              <div
                className="h-full rounded-full bg-info transition-all"
                style={{ width: `${pct ?? 0}%` }}
              />
            </div>
          </div>
          <div className="mb-3 text-xs text-ink-muted">
            stage: {job.progress?.stage || "spawning render worker"}
          </div>
        </>
      )}

      {job.status === "failed" && job.error && (
        <div className="mono mb-3 rounded-md border border-danger-border bg-danger-bg p-3 text-xs leading-relaxed text-danger">
          {job.error}
        </div>
      )}

      {job.status === "done" && job.output_path && (
        <div className="mono mb-3 truncate text-xs text-ink-muted">output: {job.output_path}</div>
      )}

      <div className="flex gap-2">
        {job.output_path && (
          <button onClick={() => void Commands.revealInFolder(job.output_path!)} className="btn btn-sm">
            Reveal file
          </button>
        )}
        {job.status === "running" && (
          <button onClick={onCancel} className="btn btn-sm">
            Cancel
          </button>
        )}
      </div>
    </div>
  );
}
