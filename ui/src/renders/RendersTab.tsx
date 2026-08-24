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
    <div className="flex-1 overflow-y-auto p-4 bg-studio-900 text-slate-100">
      <h2 className="text-lg font-bold mb-4">Renders</h2>

      {renders.length === 0 ? (
        <div className="text-sm text-slate-500 py-8 text-center">
          No renders yet. Use the Render button (or the agent's render_to_video tool) to queue one.
        </div>
      ) : (
        <>
          {/* Render queue */}
          <div className="space-y-2 mb-6">
            <h3 className="text-xs font-semibold tracking-widest text-slate-400">RENDER QUEUE</h3>
            <div className="space-y-1">
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
      return <span className="text-accent">▶</span>;
    case "done":
      return <span className="text-green-400">✓</span>;
    case "failed":
      return <span className="text-red-400">✗</span>;
    case "cancelled":
      return <span className="text-slate-500">⊘</span>;
    default:
      return <span className="text-slate-400">○</span>;
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
      className={`flex items-center justify-between p-2 rounded border cursor-pointer ${
        selected ? "bg-studio-800/50 border-studio-600" : "bg-studio-800/20 border-studio-700/50 hover:bg-studio-800/40"
      }`}
    >
      <div className="flex items-center gap-2 min-w-0">
        {statusIcon(job)}
        <span className="text-sm truncate">{label}</span>
        {job.status === "failed" && job.error && (
          <span className="text-xs text-red-400 truncate">— {job.error}</span>
        )}
      </div>
      <div className="flex items-center gap-2 text-xs shrink-0">
        {job.status === "running" && (
          <>
            <span className="bg-studio-700 px-2 py-1 rounded font-mono">
              {pct != null ? `${pct}%` : "…"}
            </span>
            {job.progress?.total_frames != null && (
              <span className="text-slate-400">
                frame {job.progress.frame}/{job.progress.total_frames}
              </span>
            )}
            <button
              onClick={(e) => {
                e.stopPropagation();
                onCancel();
              }}
              className="px-2 py-1 bg-studio-700 hover:bg-studio-600 rounded"
              title="Cancel"
            >
              ✕
            </button>
          </>
        )}
        {job.status === "queued" && <span className="text-slate-500">queued</span>}
        {job.status === "done" && job.output_path && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              void Commands.revealInFolder(job.output_path!);
            }}
            className="px-2 py-1 bg-studio-700 hover:bg-studio-600 rounded"
          >
            [reveal]
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
    <div className="border-t border-studio-800 pt-4">
      <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">
        SELECTED: {job.composition_id || job.job_id}
      </h3>
      <div className="grid grid-cols-2 gap-2 text-sm mb-4">
        <div>target: {job.target || "local"}</div>
        <div>quality: {job.quality}</div>
        <div>status: {job.status}</div>
        <div>
          {job.started_at_ms ? new Date(job.started_at_ms).toLocaleTimeString() : "—"}
        </div>
      </div>

      {(job.status === "running" || job.status === "queued") && (
        <>
          <div className="mb-2">
            <div className="flex justify-between text-xs mb-1">
              <span>progress</span>
              <span>
                {pct != null
                  ? `${pct}% · ${job.progress?.frame}/${job.progress?.total_frames}`
                  : "starting…"}
              </span>
            </div>
            <div className="w-full bg-studio-700 h-3 rounded overflow-hidden">
              <div className="bg-accent h-full transition-all" style={{ width: `${pct ?? 0}%` }} />
            </div>
          </div>
          <div className="text-xs text-slate-400 mb-2">
            stage: {job.progress?.stage || "spawning render worker"}
          </div>
        </>
      )}

      {job.status === "failed" && job.error && (
        <div className="text-sm text-red-300 bg-red-900/30 border border-red-800 rounded p-3 mb-2 font-mono text-xs">
          {job.error}
        </div>
      )}

      {job.status === "done" && job.output_path && (
        <div className="text-xs text-slate-400 mb-2">output: {job.output_path}</div>
      )}

      <div className="flex gap-2 text-xs">
        {job.output_path && (
          <button
            onClick={() => void Commands.revealInFolder(job.output_path!)}
            className="px-3 py-1 bg-studio-700 hover:bg-studio-600 rounded"
          >
            [⏓ Reveal file]
          </button>
        )}
        {job.status === "running" && (
          <button onClick={onCancel} className="px-3 py-1 bg-studio-700 hover:bg-studio-600 rounded">
            [✕ Cancel]
          </button>
        )}
      </div>
    </div>
  );
}
