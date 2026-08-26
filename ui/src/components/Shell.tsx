/**
 * Navya Studio shell — state-driven (production wiring).
 *
 * Post-onboarding layout (open-design EntryShell idiom):
 *   TopBar  ──  EntryNavRail  ──  CenterPane (Home/Projects/Models/Sources/Tools/Renders/Settings)  ──  RightRail
 *
 * Every panel reads from the Zustand store, which is fed by the Rust core
 * through #[tauri::command] + studio://event. Pickers call the real commands;
 * the home composer renders real streamed events; the status strip reflects
 * real sidecar health. No hardcoded mock content.
 */

import { useEffect } from "react";
import { useStore } from "../lib/store";
import { useKeyboardShortcuts } from "../lib/shortcuts";
import { harnessHint } from "../lib/invoke";
import { userActionLabel } from "../lib/events";
import { ApprovalDialog } from "./ApprovalDialog";
import { Inspector } from "./Inspector";
import { CommandPalette } from "./CommandPalette";
import { ShortcutsHelp } from "./ShortcutsHelp";
import { RendersTab } from "../renders/RendersTab";
import { TimelineTab } from "../timeline/TimelineTab";
import { EntryNavRail, type NavId } from "./EntryNavRail";
import { HomeView } from "./HomeView";
import { ProjectsView } from "./ProjectsView";
import { ModelsView } from "./ModelsView";
import { SourcesView } from "./SourcesView";
import { ToolsView } from "./ToolsView";
import { SettingsView } from "./SettingsView";

// --- TopBar ---
function TopBar() {
  const session = useStore((s) => s.session);
  const config = useStore((s) => s.config);
  const projects = useStore((s) => s.projects) ?? [];
  const models = useStore((s) => s.models) ?? [];
  const harnesses = useStore((s) => s.harnesses) ?? [];
  const setModel = useStore((s) => s.setModel);
  const setSource = useStore((s) => s.setSource);
  const setHarness = useStore((s) => s.setHarness);
  const openProject = useStore((s) => s.openProject);
  const render = useStore((s) => s.render);
  const abort = useStore((s) => s.abort);
  const setTheme = useStore((s) => s.setTheme);

  if (!session || !config) return null;
  const current = projects.find((p) => p.id === session.current_project_id);

  return (
    <header className="flex h-11 shrink-0 select-none items-center justify-between gap-3 border-b border-line-soft bg-canvas px-3 text-ink">
      <div className="flex min-w-0 items-center gap-3">
        <span className="whitespace-nowrap text-[13px] font-bold tracking-tight text-ink-strong">
          ⬢ Navya Studio
        </span>
        <select
          className="input input-sm w-44"
          value={session.current_project_id ?? ""}
          onChange={(e) => e.target.value && void openProject(e.target.value)}
        >
          <option value="" disabled>
            {current ? current.name : "No project open"}
          </option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </div>

      <div className="flex items-center gap-2 text-[13px]">
        <button className="btn btn-primary btn-sm" onClick={() => void render("high")}>
          Render
        </button>
        <button className="btn btn-sm" onClick={() => void abort()}>
          Stop
        </button>

        <select
          className="input input-sm w-auto"
          value={session.harness}
          onChange={(e) => void setHarness(e.target.value)}
          title={(() => {
            const h = harnesses.find((x) => x.id === session.harness);
            return h ? harnessHint(h) : "Harness";
          })()}
        >
          {harnesses.map((h) => (
            <option key={h.id} value={h.id} disabled={!h.available} title={harnessHint(h)}>
              {h.label} {h.available ? "" : "(not installed)"}
            </option>
          ))}
        </select>

        <select
          className="input input-sm w-auto"
          value={session.model}
          onChange={(e) => void setModel(e.target.value)}
          title="Model"
        >
          {models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name} {m.kind !== "chat" ? `(${m.kind})` : ""}
            </option>
          ))}
        </select>

        <div className="seg" title="Model source">
          <button
            className={session.source === "cloud" ? "is-active" : ""}
            onClick={() => void setSource("cloud")}
          >
            Cloud
          </button>
          <button
            className={session.source === "local" ? "is-active" : ""}
            onClick={() => void setSource("local")}
          >
            Local
          </button>
        </div>

        <button
          className="btn btn-sm"
          title={config.theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
          onClick={() => void setTheme(config.theme === "dark" ? "light" : "dark")}
        >
          {config.theme === "dark" ? "☀" : "☾"}
        </button>
      </div>
    </header>
  );
}

// --- RightRail ---
function RightRail() {
  const renders = useStore((s) => s.renders);
  const cancelRender = useStore((s) => s.cancelRender);
  const session = useStore((s) => s.session);
  const config = useStore((s) => s.config);
  const projects = useStore((s) => s.projects) ?? [];
  const timeline = useStore((s) => s.timeline);
  const selectedClipId = useStore((s) => s.selectedClipId);
  const sendPrompt = useStore((s) => s.sendPrompt);

  const activeCount = renders.filter((r) => r.status === "running").length;
  const doneCount = renders.filter((r) => r.status === "done").length;
  const current = projects.find((p) => p.id === session?.current_project_id);

  return (
    <aside className="w-72 shrink-0 overflow-y-auto border-l border-line-soft bg-panel p-3 text-ink">
      <div className="mb-5">
        <h3 className="rail-label">Render Queue</h3>
        <div className="mb-3 grid grid-cols-2 gap-2">
          <div
            style={{
              padding: "8px 10px",
              background: "var(--bg)",
              border: "1px solid var(--border-soft)",
              borderRadius: 8,
            }}
          >
            <div style={{ fontFamily: "var(--mono)", fontSize: 16, color: "var(--text-strong)", lineHeight: 1 }}>
              {activeCount}
            </div>
            <div style={{ fontSize: 10, color: "var(--text-faint)", marginTop: 2 }}>running</div>
          </div>
          <div
            style={{
              padding: "8px 10px",
              background: "var(--bg)",
              border: "1px solid var(--border-soft)",
              borderRadius: 8,
            }}
          >
            <div style={{ fontFamily: "var(--mono)", fontSize: 16, color: "var(--text-strong)", lineHeight: 1 }}>
              {doneCount}
            </div>
            <div style={{ fontSize: 10, color: "var(--text-faint)", marginTop: 2 }}>done</div>
          </div>
        </div>

        <div className="space-y-1.5">
          {renders.length === 0 && (
            <div className="px-2 py-1 text-xs text-ink-faint">No renders yet.</div>
          )}
          {renders.slice(0, 5).map((r) => (
            <div
              key={r.job_id}
              className="flex items-center justify-between gap-2 rounded-lg border border-line-soft bg-canvas px-2.5 py-2"
            >
              <span className="flex min-w-0 items-center gap-2 text-[13px]">
                <RenderDot status={r.status} />
                <span className="truncate">
                  <span className="mono text-xs">{r.job_id.slice(0, 8)}</span>
                  <span className="text-ink-muted"> · {r.quality}</span>
                </span>
              </span>
              <div className="flex shrink-0 gap-1 text-xs">
                {r.status === "running" && (
                  <button
                    className="icon-btn h-6 w-6"
                    title="Cancel render"
                    onClick={() => void cancelRender(r.job_id)}
                  >
                    ✕
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
      </div>

      <div>
        <h3 className="rail-label">Inspector</h3>
        <div className="space-y-1 rounded-lg border border-line-soft bg-canvas p-3">
          {selectedClipId && timeline ? (
            <Inspector
              clip={timeline.clips.find((c) => c.id === selectedClipId) ?? null}
              onEdit={(instruction) => void sendPrompt(instruction)}
            />
          ) : (
            <div className="text-[13px] font-medium text-ink-strong">
              {current ? current.name : "No project"}
            </div>
          )}
          {current && (
            <>
              <div className="mono truncate text-xs text-ink-muted" title={current.dir}>
                {current.dir}
              </div>
              <div className="text-xs text-ink-muted">
                harness <span className="text-ink">{current.harness}</span>
              </div>
              <div className="text-xs text-ink-muted">
                model <span className="text-ink">{current.model}</span>
              </div>
            </>
          )}
          {session && (
            <div className="mt-1 border-t border-line-soft pt-2">
              <div className="mb-1 text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-faint">
                Session
              </div>
              <div className="text-xs text-ink-muted">
                source <span className="text-ink">{session.source}</span>
              </div>
              <div className="text-xs text-ink-muted">
                model <span className="text-ink">{session.model}</span>
              </div>
            </div>
          )}
          {config && (
            <div className="mt-1 border-t border-line-soft pt-2">
              <div className="mb-1 text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-faint">
                Config
              </div>
              <div className="text-xs text-ink-muted">
                base <span style={{ fontFamily: "var(--mono)" }} className="text-ink">{config.navya_base_url}</span>
              </div>
              <div className="text-xs text-ink-muted">
                byok <span className="text-ink">{config.byok ? "yes" : "no"}</span>
              </div>
            </div>
          )}
        </div>
      </div>
    </aside>
  );
}

function RenderDot({ status }: { status: string }) {
  const cls =
    status === "running"
      ? "text-info"
      : status === "done"
        ? "text-ok"
        : status === "failed"
          ? "text-danger"
          : "text-ink-faint";
  const glyph = status === "running" ? "▶" : status === "done" ? "✓" : status === "failed" ? "✗" : "○";
  return <span className={`text-xs ${cls}`}>{glyph}</span>;
}

// --- StatusStrip ---
function StatusStrip() {
  const sidecars = useStore((s) => s.sidecars) ?? [];
  const error = useStore((s) => s.error);
  const studioError = useStore((s) => s.studioError);
  const dismissError = useStore((s) => s.dismissError);
  // Live render count from the renders array (updated by events), not the
  // one-shot snapshot value which is stale after the initial load.
  const renderCount = useStore((s) => s.renders?.length ?? 0);

  const dot = (status: string) =>
    status === "running" ? "text-info" : status === "exited" ? "text-danger" : "text-ink-faint";

  return (
    <footer className="flex h-7 shrink-0 items-center gap-4 border-t border-line-soft bg-panel px-3 text-xs text-ink-muted">
      {sidecars.map((sc) => (
        <span key={sc.name} className="flex items-center gap-1.5" title={sc.detail ?? sc.status}>
          <span className={`text-[9px] ${dot(sc.status)}`}>●</span>
          <span>
            {sc.name} <span className="text-ink-faint">{sc.status}</span>
          </span>
        </span>
      ))}
      <span className="numeric ml-auto text-ink-muted">{renderCount} render(s)</span>
      {studioError && (
        <span
          className="flex items-center gap-2 rounded border border-danger-border bg-danger-bg px-2 py-0.5 text-danger"
          title={studioError.code}
        >
          <span className="max-w-xs truncate">⚠ {studioError.message}</span>
          <span className="text-ink-faint">— {userActionLabel(studioError.user_action)}</span>
          <button
            className="icon-btn h-4 w-4 shrink-0 text-danger"
            title="Dismiss"
            onClick={() => dismissError()}
          >
            ×
          </button>
        </span>
      )}
      {error && <span className="max-w-xs truncate text-danger">⚠ {error}</span>}
    </footer>
  );
}

// --- StudioShell (top-level) ---
export function StudioShell() {
  const navId = useStore((s) => s.navId);
  const setNavId = useStore((s) => s.setNavId);
  const railsHidden = useStore((s) => s.railsHidden);
  const refreshRenders = useStore((s) => s.refreshRenders);
  const pendingApproval = useStore((s) => s.pendingApproval);
  const approve = useStore((s) => s.approve);

  // Global keyboard shortcuts (Phase 16): ⌘K palette, tab/rail toggles,
  // timeline shuttle, etc. Attaches one window keydown listener.
  useKeyboardShortcuts();

  useEffect(() => {
    void refreshRenders();
  }, [refreshRenders]);

  const navigate = (id: NavId) => setNavId(id);

  return (
    <div className="flex h-screen flex-col bg-canvas text-ink">
      <TopBar />
      <div className="flex flex-1 overflow-hidden">
        {!railsHidden.left && <EntryNavRail navId={navId} onChange={setNavId} />}
        <main className="entry-main">
          <div className="entry-main__scroll">
            {navId === "home" && <HomeView onNavigate={navigate} />}
            {navId === "projects" && <ProjectsView />}
            {navId === "timeline" && <TimelineTab />}
            {navId === "models" && <ModelsView />}
            {navId === "sources" && <SourcesView />}
            {navId === "tools" && <ToolsView />}
            {navId === "renders" && <RendersTab />}
            {navId === "settings" && <SettingsView />}
          </div>
        </main>
        {!railsHidden.right && <RightRail />}
      </div>
      <StatusStrip />
      {pendingApproval && (
        <ApprovalDialog
          request={pendingApproval}
          onAnswer={(answer, alwaysAllow) => {
            void approve(answer, alwaysAllow);
          }}
        />
      )}
      <CommandPalette />
      <ShortcutsHelp />
    </div>
  );
}
