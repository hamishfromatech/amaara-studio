/**
 * ToolsView — surface for sidecars, MCP tools, and render worker health.
 *
 * Pulls live sidecar status from the store and renders each as a row in a
 * list card. Cheap, honest: only shows what the studio actually has
 * wired up. The render-worker subprocess is the most important one for
 * users to see — its presence/absence drives whether "Render" can do
 * anything.
 */

import { useStore } from "../lib/store";

function statusTone(s: string): string {
  if (s === "running" || s === "loaded" || s === "ready") return "text-ok";
  if (s === "starting" || s === "stopped") return "text-info";
  if (s === "exited" || s === "failed") return "text-danger";
  return "text-ink-faint";
}

export function ToolsView() {
  const sidecars = useStore((s) => s.sidecars) ?? [];

  return (
    <div className="entry-main__scroll-inner">
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Sidecars</h2>
          <div className="entry-section__actions">
            <span style={{ color: "var(--text-faint)", fontFamily: "var(--mono)", fontSize: 10 }}>
              {sidecars.length} registered
            </span>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {sidecars.length === 0 && (
              <div style={{ padding: "14px 18px", color: "var(--text-faint)", fontSize: 12 }}>
                No sidecars registered yet.
              </div>
            )}
            {sidecars.map((sc) => (
              <div key={sc.name} className="list-card__row">
                <span
                  aria-hidden
                  className={statusTone(sc.status)}
                  style={{ fontSize: 10 }}
                >
                  ●
                </span>
                <span className="list-card__row-title">{sc.name}</span>
                <span
                  className={`list-card__row-meta ${statusTone(sc.status)}`}
                  title={sc.detail ?? sc.status}
                >
                  {sc.status}
                </span>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">MCP Tools</h2>
          <div className="entry-section__actions">
            <span style={{ color: "var(--text-faint)", fontSize: 11 }}>
              exposed over stdio to MCP-capable harnesses
            </span>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {[
              { id: "generate_image", kind: "image" },
              { id: "render_to_video", kind: "video" },
              { id: "list_local_models", kind: "meta" },
              { id: "set_generation_source", kind: "meta" },
            ].map((tool) => (
              <div key={tool.id} className="list-card__row">
                <span aria-hidden style={{ fontSize: 10, color: "var(--text-faint)" }}>⌬</span>
                <span className="list-card__row-title" style={{ fontFamily: "var(--mono)" }}>
                  {tool.id}
                </span>
                <span className="list-card__row-meta">{tool.kind}</span>
              </div>
            ))}
          </div>
        </div>
      </section>
    </div>
  );
}
