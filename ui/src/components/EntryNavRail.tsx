/**
 * Labeled nav rail — the post-onboarding navigation surface.
 *
 * Borrows the `EntryNavRail` vocabulary from open-design: a 220px column
 * with icons + labels grouped under section headings, a hairline divider
 * between groups, and a quiet footer. Each item shows an optional count
 * badge (e.g. render queue length, project count).
 *
 * The rail is purely navigation — content lives in the center pane. The
 * Shell owns which item is active and renders the matching view.
 */

import { useStore } from "../lib/store";

export type NavId =
  | "home"
  | "projects"
  | "models"
  | "sources"
  | "tools"
  | "renders"
  | "settings";

interface NavItem {
  id: NavId;
  label: string;
  glyph: string;
  /** Optional count badge content (e.g. number, dot). */
  badge?: string;
  /** Show a small brand-green dot to the right of the label. */
  dot?: boolean;
}

interface NavGroup {
  label: string;
  items: NavItem[];
}

interface Props {
  navId: NavId;
  onChange: (id: NavId) => void;
}

export function EntryNavRail({ navId, onChange }: Props) {
  const projects = useStore((s) => s.projects) ?? [];
  const renders = useStore((s) => s.renders) ?? [];
  const session = useStore((s) => s.session);
  const hasApiKey = useStore((s) => s.has_api_key) ?? false;

  const runningRenders = renders.filter((r) => r.status === "running").length;

  const groups: NavGroup[] = [
    {
      label: "Workspace",
      items: [
        { id: "home", label: "Home", glyph: "◐" },
        {
          id: "projects",
          label: "Projects",
          glyph: "▤",
          badge: projects.length ? String(projects.length) : undefined,
        },
        {
          id: "renders",
          label: "Renders",
          glyph: "▷",
          badge: runningRenders ? String(runningRenders) : renders.length ? String(renders.length) : undefined,
          dot: runningRenders > 0,
        },
      ],
    },
    {
      label: "Library",
      items: [
        { id: "models", label: "Models", glyph: "◆" },
        { id: "sources", label: "Sources", glyph: "⇄" },
        { id: "tools", label: "Tools", glyph: "⌬" },
      ],
    },
    {
      label: "App",
      items: [
        {
          id: "settings",
          label: "Settings",
          glyph: "⚙",
          dot: !hasApiKey,
        },
      ],
    },
  ];

  const activeProjectName =
    projects.find((p) => p.id === session?.current_project_id)?.name ?? null;

  return (
    <nav className="entry-nav-rail" aria-label="Studio navigation">
      <div className="entry-nav-rail__brand">
        <span className="entry-nav-rail__brand-mark" aria-hidden>⬢</span>
        <span className="entry-nav-rail__brand-name">Navya Studio</span>
      </div>

      <div className="entry-nav-rail__scroll">
        {groups.map((group, gi) => (
          <div key={group.label} className="entry-nav-rail__group">
            {gi > 0 && <div className="entry-nav-rail__divider" />}
            <div className="entry-nav-rail__group-label">{group.label}</div>
            {group.items.map((item) => {
              const isActive = navId === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  className={`entry-nav-rail__btn ${isActive ? "is-active" : ""}`}
                  aria-current={isActive ? "page" : undefined}
                  onClick={() => onChange(item.id)}
                >
                  <span className="entry-nav-rail__btn-icon" aria-hidden>
                    {item.glyph}
                  </span>
                  <span className="entry-nav-rail__btn-label">{item.label}</span>
                  {item.dot && <span className="entry-nav-rail__btn-dot" aria-hidden />}
                  {item.badge && (
                    <span className="entry-nav-rail__btn-count">{item.badge}</span>
                  )}
                </button>
              );
            })}
          </div>
        ))}
      </div>

      <div className="entry-nav-rail__footer">
        {activeProjectName ? (
          <div className="entry-nav-rail__footer-row">
            <span>Project</span>
            <span style={{ color: "var(--text)", fontFamily: "var(--mono)", fontSize: 10 }}>
              {activeProjectName}
            </span>
          </div>
        ) : (
          <div className="entry-nav-rail__footer-row">
            <span>No project open</span>
          </div>
        )}
        <div className="entry-nav-rail__footer-row" style={{ marginTop: 4 }}>
          <span>{hasApiKey ? "Connected" : "API key missing"}</span>
          <span
            aria-hidden
            style={{
              display: "inline-block",
              width: 6,
              height: 6,
              borderRadius: "50%",
              background: hasApiKey ? "var(--brand)" : "var(--amber)",
            }}
          />
        </div>
      </div>
    </nav>
  );
}
