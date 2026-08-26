/**
 * HomeView — the post-onboarding landing surface.
 *
 * Composition (open-design HomeView + HomeHero + RecentProjectsStrip idiom):
 *   1. Centered hero block on a faint dot wash:
 *        wordmark, tagline, row of scenario pills (TypePillRow equivalent),
 *        composer card with active-attachment chip row above the textarea,
 *        send bar (Send / Steer / ⌘↵ hint).
 *   2. If the chat has messages, the most recent few float above the hero
 *      so users see the live conversation without leaving Home.
 *   3. Stacked `entry-section` blocks below: Recent Projects (horizontal
 *      strip), Active Renders (compact queue summary), Tools (MCP /
 *      render-worker pills).
 *
 * The composer here IS the chat input — selecting Home never strands the
 * user; the old Chat tab is subsumed by Home.
 */

import { useEffect, useRef, useState } from "react";
import { useStore } from "../lib/store";
import { formatRelative, swatchFor } from "../lib/format";

interface ScenarioPill {
  id: string;
  label: string;
  glyph: string;
  /** Seed text inserted into the composer when the pill is clicked. */
  seed: string;
}

const SCENARIO_PILLS: ScenarioPill[] = [
  { id: "render", label: "Render", glyph: "▷", seed: "Render the current composition to MP4." },
  { id: "storyboard", label: "Storyboard", glyph: "▤", seed: "Write a 6-shot storyboard for: " },
  { id: "edit", label: "Edit", glyph: "✎", seed: "Edit the current composition: " },
  { id: "brand", label: "Brand", glyph: "✸", seed: "Apply brand tokens and re-style: " },
  { id: "analyze", label: "Analyze", glyph: "◌", seed: "Analyze the current composition and suggest improvements: " },
];

export function HomeView({ onNavigate }: { onNavigate: (id: "projects" | "models" | "sources" | "tools" | "renders" | "settings") => void }) {
  const session = useStore((s) => s.session);
  const projects = useStore((s) => s.projects) ?? [];
  const renders = useStore((s) => s.renders) ?? [];
  const harnesses = useStore((s) => s.harnesses) ?? [];
  const sidecars = useStore((s) => s.sidecars) ?? [];
  const chat = useStore((s) => s.chat);
  const sendPrompt = useStore((s) => s.sendPrompt);
  const steer = useStore((s) => s.steer);
  const openProject = useStore((s) => s.openProject);
  const refreshRenders = useStore((s) => s.refreshRenders);

  const current = projects.find((p) => p.id === session?.current_project_id);

  const [text, setText] = useState("");
  const [activePill, setActivePill] = useState<string | null>(null);
  const taRef = useRef<HTMLTextAreaElement | null>(null);

  // Keep renders fresh on Home view; the StatusStrip already calls this on
  // mount, but we want the active-renders section to update promptly too.
  useEffect(() => {
    void refreshRenders();
  }, [refreshRenders]);

  const pickPill = (pill: ScenarioPill) => {
    setActivePill(pill.id);
    setText(pill.seed);
    // Focus the textarea so the user can immediately edit.
    requestAnimationFrame(() => taRef.current?.focus());
  };

  const send = (mode: "normal" | "steer" | "follow_up") => {
    const msg = text.trim();
    if (!msg) return;
    if (mode === "steer") {
      void steer(msg);
    } else {
      void sendPrompt(msg, mode === "follow_up" ? "follow_up" : "normal");
    }
    setText("");
    setActivePill(null);
  };

  // Show at most the last 4 messages above the composer — long sessions
  // still have the full ChatView surface area in the home page.
  const recentMessages = (chat ?? []).slice(-4);

  const activeRenders = renders
    .filter((r) => r.status === "running" || r.status === "queued")
    .slice(0, 3);

  // Tool pills are sourced from real sidecar health — when a worker is up,
  // it shows as an enabled pill; offline workers show muted with their last
  // status. Cheap and truthful.
  const toolsPills = [
    { id: "render-worker", label: "render-worker", detail: sidecars.find((s) => s.name.includes("render"))?.status ?? "idle" },
    { id: "mcp", label: "mcp tools", detail: "stdio" },
    { id: "skill", label: "skills", detail: "loaded" },
  ];

  return (
    <div className="home-wash entry-main__scroll-inner">
      <div className="home-hero">
        <h1 className="home-hero__logo">Navya Studio</h1>
        <p className="home-hero__tagline">
          Direct an agent. Watch it make. Render to video.
        </p>

        <div className="home-pill-row" role="tablist" aria-label="Scenarios">
          {SCENARIO_PILLS.map((pill) => (
            <button
              key={pill.id}
              type="button"
              role="tab"
              aria-selected={activePill === pill.id}
              className={`home-pill ${activePill === pill.id ? "is-active" : ""}`}
              onClick={() => pickPill(pill)}
            >
              <span className="home-pill__glyph" aria-hidden>{pill.glyph}</span>
              {pill.label}
            </button>
          ))}
        </div>

        {recentMessages.length > 0 && (
          <div className="home-hero__history" aria-label="Recent conversation">
            {recentMessages.map((m) => (
              <div
                key={m.id}
                className={`home-hero__history-msg home-hero__history-msg--${m.role} ${
                  m.status === "thinking" ? "home-hero__history-msg--thinking" : ""
                } ${m.status === "error" ? "home-hero__history-msg--error" : ""}`}
              >
                <div className="home-hero__history-msg-head">
                  <span>
                    {m.role === "you"
                      ? "You"
                      : m.role === "agent"
                        ? `Agent${session?.harness ? ` · ${session.harness}` : ""}`
                        : "System"}
                  </span>
                  {m.status === "thinking" && <span>· thinking…</span>}
                  {m.status === "tool-calling" && <span>· tool…</span>}
                  {m.status === "done" && <span>· done</span>}
                  {m.status === "error" && <span>· error</span>}
                </div>
                <div className="home-hero__history-msg-body">
                  {m.content || (m.status === "thinking" ? "…" : "")}
                </div>
              </div>
            ))}
          </div>
        )}

        <div className="home-hero__composer-card">
          <div className="home-hero__active-row">
            {current ? (
              <span className="home-active-chip" title={`harness ${current.harness} · model ${current.model}`}>
                <span aria-hidden style={{ fontSize: 10, color: "var(--brand)" }}>●</span>
                <span className="home-active-chip__label">{current.name}</span>
                <span className="home-active-chip__meta">{current.harness}</span>
              </span>
            ) : (
              <span className="home-active-chip home-active-chip--placeholder">
                <span className="home-active-chip__label">No project open — pick or create one in Projects</span>
              </span>
            )}
            <span className="home-active-chip">
              <span aria-hidden style={{ fontSize: 10, color: "var(--text-faint)" }}>◇</span>
              <span className="home-active-chip__label">{session?.model ?? "model"}</span>
            </span>
          </div>

          <textarea
            ref={taRef}
            className="home-hero__composer"
            placeholder={
              current
                ? "Describe what you want to make. The agent will author the composition and render it."
                : "Open a project to enable the agent."
            }
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              // ⌘Enter send · ⇧⌘Enter steer · ⌘⌥Enter queue follow-up.
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                if (e.altKey) send("follow_up");
                else send(e.shiftKey ? "steer" : "normal");
              }
            }}
            rows={3}
            disabled={!current}
          />

          <div className="home-hero__send-bar">
            <button
              className="btn btn-primary btn-sm"
              onClick={() => send("normal")}
              disabled={!text.trim() || !current}
            >
              Send
            </button>
            <button
              className="btn btn-ghost btn-sm"
              onClick={() => send("steer")}
              disabled={!text.trim() || !current}
              title="Steer the agent without breaking the current turn"
            >
              Steer
            </button>
            <span className="home-hero__send-bar-spacer" />
            <span className="home-hero__hint">⌘↵ send · ⇧⌘↵ steer · ⌘⌥↵ follow-up</span>
          </div>
        </div>
      </div>

      {/* Recent Projects — horizontal strip */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Recent Projects</h2>
          <div className="entry-section__actions">
            <button className="entry-section__action" onClick={() => onNavigate("projects")}>
              View all →
            </button>
          </div>
        </div>
        <div className="recent-projects-strip" role="list">
          {projects.length === 0 && (
            <div className="recent-projects-strip__empty">
              No projects yet. Create one to start working.
            </div>
          )}
          {projects.slice(0, 8).map((p) => (
            <button
              key={p.id}
              type="button"
              role="listitem"
              className="recent-project-card"
              onClick={() => void openProject(p.id)}
            >
              <div className="recent-project-card__swatch" style={{ background: swatchFor(p.id) }} />
              <div className="recent-project-card__name" title={p.name}>{p.name}</div>
              <div className="recent-project-card__meta">
                <span className="recent-project-card__dir" title={p.dir}>{p.dir}</span>
                <span>{formatRelative(p.created_at_ms)}</span>
              </div>
            </button>
          ))}
          <button
            type="button"
            className="recent-projects-strip__new-card"
            onClick={() => onNavigate("projects")}
          >
            <span className="recent-projects-strip__new-card-glyph" aria-hidden>+</span>
            New project
          </button>
        </div>
      </section>

      {/* Active Renders — compact queue summary */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Active Renders</h2>
          <div className="entry-section__actions">
            <button className="entry-section__action" onClick={() => onNavigate("renders")}>
              View queue →
            </button>
          </div>
        </div>
        {activeRenders.length === 0 ? (
          <div className="list-card">
            <div className="list-card__body" style={{ padding: "14px 18px", color: "var(--text-faint)", fontSize: 12 }}>
              No renders running. Hit <span style={{ fontFamily: "var(--mono)" }}>Render</span> from the top bar
              to start one.
            </div>
          </div>
        ) : (
          <div className="list-card">
            <div className="list-card__body">
              {activeRenders.map((r) => (
                <div key={r.job_id} className="list-card__row">
                  <span style={{ color: "var(--info)", fontSize: 11 }} aria-hidden>▶</span>
                  <span className="list-card__row-title">
                    <span style={{ fontFamily: "var(--mono)", fontSize: 11 }}>{r.job_id.slice(0, 8)}</span>
                    <span style={{ color: "var(--text-muted)" }}> · {r.quality || "draft"}</span>
                  </span>
                  <span className="list-card__row-meta">{r.target || "mp4"}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </section>

      {/* Tools */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Tools</h2>
          <div className="entry-section__actions">
            <button className="entry-section__action" onClick={() => onNavigate("tools")}>
              Configure →
            </button>
          </div>
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
          {toolsPills.map((t) => {
            const up = t.detail === "running" || t.detail === "loaded";
            return (
              <span
                key={t.id}
                className="chip"
                style={{
                  opacity: up ? 1 : 0.55,
                  cursor: "default",
                }}
                title={`${t.label} · ${t.detail}`}
              >
                <span
                  aria-hidden
                  style={{
                    fontSize: 9,
                    color: up ? "var(--brand)" : "var(--text-faint)",
                  }}
                >
                  ●
                </span>
                {t.label}
                <span
                  style={{
                    fontFamily: "var(--mono)",
                    fontSize: 10,
                    color: "var(--text-soft)",
                    marginLeft: 4,
                  }}
                >
                  {t.detail}
                </span>
              </span>
            );
          })}
          <span
            className="chip"
            style={{ cursor: "default", opacity: 0.85 }}
            title="Active harness"
          >
            {harnesses.find((h) => h.id === session?.harness)?.label ?? session?.harness ?? "harness"}
          </span>
        </div>
      </section>
    </div>
  );
}
