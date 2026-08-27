/**
 * Command palette (Phase 16 — ⌘K / Ctrl+K).
 *
 * A centered, filterable launcher for the things you reach for by hand:
 * switch project, jump to a center tab, and run common actions (new project,
 * render, stop, toggle rails). Arrow keys step the highlight, Enter runs it,
 * Esc closes. State (open flag, projects) lives in the store so the global
 * shortcuts hook can toggle it from anywhere.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "../lib/store";
import { Commands, type ProjectRow } from "../lib/invoke";
import { NAV_LABELS, type NavId } from "../lib/nav";

interface PaletteItem {
  id: string;
  group: string;
  label: string;
  hint?: string;
  run: () => void;
}

const TAB_NAV: { id: NavId; hint?: string }[] = [
  { id: "home", hint: "⌘1" },
  { id: "timeline", hint: "⌘2" },
  { id: "renders", hint: "⌘3" },
  { id: "models", hint: "⌘4" },
  { id: "projects" },
  { id: "sources" },
  { id: "tools" },
  { id: "settings", hint: "⌘," },
];

export function CommandPalette() {
  const open = useStore((s) => s.paletteOpen);
  const setPaletteOpen = useStore((s) => s.setPaletteOpen);
  const openProject = useStore((s) => s.openProject);
  const setNavId = useStore((s) => s.setNavId);
  const render = useStore((s) => s.render);
  const abort = useStore((s) => s.abort);
  const newProject = useStore((s) => s.newProject);
  const toggleLeftRail = useStore((s) => s.toggleLeftRail);
  const toggleRightRail = useStore((s) => s.toggleRightRail);
  const currentProjectId = useStore((s) => s.session?.current_project_id);

  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    void Commands.listProjects().then(setProjects).catch(() => setProjects([]));
    // Focus the filter input once the overlay mounts.
    const t = window.setTimeout(() => inputRef.current?.focus(), 0);
    return () => window.clearTimeout(t);
  }, [open]);

  // Reset selection + query whenever the overlay opens.
  useEffect(() => {
    if (open) {
      setQuery("");
      setActive(0);
    }
  }, [open]);

  const items: PaletteItem[] = useMemo(() => {
    const projItems: PaletteItem[] = projects.map((p) => ({
      id: `proj:${p.id}`,
      group: "Projects",
      label: p.name,
      hint: p.id === currentProjectId ? "active" : p.id.slice(0, 8),
      run: () => {
        void openProject(p.id);
        setPaletteOpen(false);
      },
    }));

    const tabItems: PaletteItem[] = TAB_NAV.map((t) => ({
      id: `tab:${t.id}`,
      group: "Tabs",
      label: NAV_LABELS[t.id],
      hint: t.hint,
      run: () => {
        setNavId(t.id);
        setPaletteOpen(false);
      },
    }));

    const actionItems: PaletteItem[] = [
      {
        id: "action:new",
        group: "Actions",
        label: "New project",
        hint: "⌘N",
        run: () => {
          setPaletteOpen(false);
          void (async () => {
            const name = window.prompt("Project name");
            if (!name?.trim()) return;
            const dir = window.prompt("Project directory (absolute path)", ".");
            if (dir == null || !dir.trim()) return;
            try {
              await newProject(name.trim(), dir.trim());
            } catch {
              /* surfaces via the status strip */
            }
          })();
        },
      },
      {
        id: "action:render",
        group: "Actions",
        label: "Render current composition",
        hint: "⌘R",
        run: () => {
          void render("draft");
          setPaletteOpen(false);
        },
      },
      {
        id: "action:stop",
        group: "Actions",
        label: "Stop agent turn",
        hint: "⌘.",
        run: () => {
          void abort();
          setPaletteOpen(false);
        },
      },
      {
        id: "action:left-rail",
        group: "Actions",
        label: "Toggle left rail",
        hint: "⌘\\",
        run: () => {
          toggleLeftRail();
          setPaletteOpen(false);
        },
      },
      {
        id: "action:right-rail",
        group: "Actions",
        label: "Toggle right rail",
        hint: "⌘/",
        run: () => {
          toggleRightRail();
          setPaletteOpen(false);
        },
      },
    ];

    return [...projItems, ...tabItems, ...actionItems];
  }, [
    projects,
    currentProjectId,
    openProject,
    setNavId,
    render,
    abort,
    newProject,
    toggleLeftRail,
    toggleRightRail,
    setPaletteOpen,
  ]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const groups = new Map<string, PaletteItem[]>();
    for (const it of items) {
      if (q && !it.label.toLowerCase().includes(q) && !it.group.toLowerCase().includes(q)) continue;
      const arr = groups.get(it.group) ?? [];
      arr.push(it);
      groups.set(it.group, arr);
    }
    return groups;
  }, [items, query]);

  // Flatten for arrow-key stepping (only into items that pass the filter).
  const visible = useMemo(() => Object.values(filtered).flat(), [filtered]);

  if (!open) return null;

  const handleKey = (e: React.KeyboardEvent) => {
    if (e.nativeEvent.isComposing) return; // IME guard (CJK composition)
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => (a + 1) % Math.max(1, visible.length));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => (a - 1 + visible.length) % Math.max(1, visible.length));
    } else if (e.key === "Enter") {
      const item = visible[active];
      if (item) {
        e.preventDefault();
        item.run();
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      setPaletteOpen(false);
    }
  };

  // Keep the highlight in view while stepping.
  useEffect(() => {
    const node = listRef.current?.children[active] as HTMLElement | undefined;
    node?.scrollIntoView({ block: "nearest" });
  }, [active]);

  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && setPaletteOpen(false)}>
      <div
        className="modal flex max-w-lg flex-col lg:w-1/2"
        onKeyDown={handleKey}
        role="dialog"
        aria-label="Command palette"
      >
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          placeholder="Type to search projects, tabs, and actions…"
          className="border-0 border-b border-line-soft bg-canvas px-4 py-3 text-sm text-ink outline-none"
        />
        <div ref={listRef} className="max-h-80 overflow-y-auto p-2">
          {visible.length === 0 ? (
            <div className="px-3 py-6 text-center text-sm text-ink-faint">Nothing matches “{query}”.</div>
          ) : (
            Object.entries(filtered).map(([group, groupItems]) => (
              <div key={group} className="mt-2 first:mt-0">
                <div className="px-2 pb-1 text-[10px] font-semibold uppercase tracking-wide text-ink-faint">
                  {group}
                </div>
                {groupItems.map((it: PaletteItem) => (
                  <button
                    key={it.id}
                    className={`flex w-full items-center justify-between gap-3 rounded-md px-2 py-1.5 text-left text-sm ${
                      active === visible.indexOf(it) ? "bg-info/20 text-ink-strong" : "text-ink"
                    }`}
                    onMouseEnter={() => setActive(visible.indexOf(it))}
                    onClick={() => it.run()}
                  >
                    <span>{it.label}</span>
                    {it.hint && <span className="mono text-xs text-ink-faint">{it.hint}</span>}
                  </button>
                ))}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

