/**
 * Keyboard-shortcuts help overlay (Phase 16 — `?`).
 *
 * Lists the global hotkeys from `design.md` §10, grouped. Rendered only while
 * `shortcutsOpen` is set; the shortcuts hook toggles that flag on `?`.
 */

import { useStore } from "../lib/store";

const GROUPS: { title: string; rows: [string, string][] }[] = [
  {
    title: "Palette & help",
    rows: [
      ["⌘K / Ctrl+K", "Command palette"],
      ["?", "Show this overlay"],
    ],
  },
  {
    title: "Navigation",
    rows: [
      ["⌘1 … ⌘4", "Chat · Timeline · Renders · Models"],
      ["⌘N", "New project"],
      ["⌘,", "Settings"],
      ["⌘\\", "Toggle left rail"],
      ["⌘/", "Toggle right rail"],
    ],
  },
  {
    title: "Chat",
    rows: [
      ["⌘Enter", "Send (normal)"],
      ["⌘⇧Enter", "Send as steer"],
      ["⌘⌥Enter", "Send as follow-up"],
      ["⌘.", "Stop current turn"],
      ["⌘E", "Edit selected clip in chat"],
    ],
  },
  {
    title: "Timeline",
    rows: [
      ["Space", "Play / pause"],
      ["J · L", "Shuttle back / forward"],
      ["⌘R", "Render current composition"],
    ],
  },
];

export function ShortcutsHelp() {
  const open = useStore((s) => s.shortcutsOpen);
  const setShortcutsOpen = useStore((s) => s.setShortcutsOpen);

  if (!open) return null;

  return (
    <div
      className="scrim"
      onMouseDown={(e) => e.target === e.currentTarget && setShortcutsOpen(false)}
    >
      <div className="modal max-w-md" role="dialog" aria-label="Keyboard shortcuts">
        <div className="mb-3 flex items-center justify-between">
          <h3 className="text-sm font-semibold text-ink-strong">Keyboard shortcuts</h3>
          <button
            className="icon-btn h-6 w-6"
            title="Close"
            onClick={() => setShortcutsOpen(false)}
          >
            ×
          </button>
        </div>
        <div className="max-h-80 grid grid-cols-2 gap-x-6 gap-y-3 overflow-y-auto">
          {GROUPS.map((g) => (
            <div key={g.title}>
              <div className="mb-1 text-[10px] font-semibold uppercase tracking-wide text-ink-faint">
                {g.title}
              </div>
              {g.rows.map(([key, label]) => (
                <div key={key} className="flex items-center justify-between gap-3 text-xs">
                  <span className="text-ink-muted">{label}</span>
                  <span className="mono text-ink-strong">{key}</span>
                </div>
              ))}
            </div>
          ))}
        </div>
        <div className="mt-4 text-center text-[11px] text-ink-faint">Press ? or Esc to close</div>
      </div>
    </div>
  );
}
