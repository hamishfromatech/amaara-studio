/**
 * SourcesView — explainer for the cloud/local source toggle.
 *
 * The TopBar already has the `Cloud / Local` segmented control; this
 * page documents what each source means and which harnesses/models are
 * available there, so users landing here get the context they came for.
 */

import { useStore } from "../lib/store";

export function SourcesView() {
  const source = useStore((s) => s.session?.source ?? "cloud");
  const setSource = useStore((s) => s.setSource);
  const hasApiKey = useStore((s) => s.has_api_key) ?? false;

  return (
    <div className="entry-main__scroll-inner entry-main__scroll-inner--narrow">
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Generation Source</h2>
          <div className="entry-section__actions">
            <span style={{ color: "var(--text-faint)", fontFamily: "var(--mono)", fontSize: 10 }}>
              current · {source}
            </span>
          </div>
        </div>

        <div style={{ display: "flex", gap: 12 }}>
          <button
            type="button"
            className={`list-card`}
            onClick={() => void setSource("cloud")}
            style={{
              flex: 1,
              textAlign: "left",
              padding: 14,
              cursor: "pointer",
              borderColor: source === "cloud" ? "var(--border-selected)" : undefined,
              background: source === "cloud" ? "color-mix(in srgb, var(--selected) 6%, var(--bg))" : undefined,
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
              <span style={{ fontSize: 14, color: source === "cloud" ? "var(--brand)" : "var(--text-faint)" }}>●</span>
              <span style={{ fontSize: 14, fontWeight: 600, color: "var(--text-strong)" }}>Cloud (Navya)</span>
            </div>
            <p style={{ margin: 0, fontSize: 12, color: "var(--text-muted)", lineHeight: 1.6 }}>
              Render on Navya-managed GPUs. Requires an API key. Highest fidelity and
              the broadest model catalog.
            </p>
            <div style={{ marginTop: 10, fontFamily: "var(--mono)", fontSize: 10, color: "var(--text-soft)" }}>
              {hasApiKey ? "✓ connected" : "✗ no api key"}
            </div>
          </button>

          <button
            type="button"
            className={`list-card`}
            onClick={() => void setSource("local")}
            style={{
              flex: 1,
              textAlign: "left",
              padding: 14,
              cursor: "pointer",
              borderColor: source === "local" ? "var(--border-selected)" : undefined,
              background: source === "local" ? "color-mix(in srgb, var(--selected) 6%, var(--bg))" : undefined,
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
              <span style={{ fontSize: 14, color: source === "local" ? "var(--brand)" : "var(--text-faint)" }}>●</span>
              <span style={{ fontSize: 14, fontWeight: 600, color: "var(--text-strong)" }}>Local</span>
            </div>
            <p style={{ margin: 0, fontSize: 12, color: "var(--text-muted)", lineHeight: 1.6 }}>
              Render on this machine via the SD.cpp sidecar. Lower fidelity, no API key,
              but private and offline-friendly.
            </p>
            <div style={{ marginTop: 10, fontFamily: "var(--mono)", fontSize: 10, color: "var(--text-soft)" }}>
              SD sidecar: see Tools
            </div>
          </button>
        </div>
      </section>
    </div>
  );
}
