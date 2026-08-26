/**
 * SettingsView — API key + harness picker.
 *
 * Two real settings the studio exposes today: the Navya cloud API key
 * (required for cloud generation), and the active harness (which CLI
 * the studio spawns). Each is a list card with rows for the available
 * options. Keeps the surface quiet and honest — no toggles that don't
 * do anything yet.
 */

import { useState } from "react";
import { useStore } from "../lib/store";
import { harnessHint } from "../lib/invoke";

export function SettingsView() {
  const config = useStore((s) => s.config);
  const harnesses = useStore((s) => s.harnesses) ?? [];
  const session = useStore((s) => s.session);
  const saveApiKey = useStore((s) => s.saveApiKey);
  const clearApiKey = useStore((s) => s.clearApiKey);
  const setHarness = useStore((s) => s.setHarness);
  const setTheme = useStore((s) => s.setTheme);
  const setDensity = useStore((s) => s.setDensity);
  const setShareAnalytics = useStore((s) => s.setShareAnalytics);

  const [keyInput, setKeyInput] = useState("");

  const hasApiKey = useStore((s) => s.has_api_key) ?? false;

  return (
    <div className="entry-main__scroll-inner entry-main__scroll-inner--narrow">
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Navya Cloud API Key</h2>
          <div className="entry-section__actions">
            <span
              style={{
                fontFamily: "var(--mono)",
                fontSize: 10,
                color: hasApiKey ? "var(--green)" : "var(--amber)",
              }}
            >
              {hasApiKey ? "● connected" : "● missing"}
            </span>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {!hasApiKey && (
              <form
                onSubmit={async (e) => {
                  e.preventDefault();
                  if (!keyInput.trim()) return;
                  await saveApiKey(keyInput.trim());
                  setKeyInput("");
                }}
                style={{ display: "flex", gap: 8, padding: 12 }}
              >
                <input
                  className="input input-sm"
                  placeholder="navya-…"
                  value={keyInput}
                  onChange={(e) => setKeyInput(e.target.value)}
                  type="password"
                  style={{ flex: 1 }}
                  autoFocus
                />
                <button
                  type="submit"
                  className="btn btn-primary btn-sm"
                  disabled={!keyInput.trim()}
                >
                  Save
                </button>
              </form>
            )}
            {hasApiKey && (
              <div className="list-card__row">
                <span className="list-card__row-title">
                  <span style={{ fontFamily: "var(--mono)" }}>navya-••••••••</span>
                </span>
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={async () => {
                    await clearApiKey();
                    setKeyInput("");
                  }}
                >
                  Clear
                </button>
              </div>
            )}
          </div>
        </div>
        <p style={{ marginTop: 8, fontSize: 11, color: "var(--text-faint)", lineHeight: 1.6 }}>
          Stored locally in the OS keychain. Used only for cloud renders and model
          catalog sync. Clear it any time.
        </p>
      </section>

      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Harness</h2>
          <div className="entry-section__actions">
            <span style={{ color: "var(--text-faint)", fontSize: 11 }}>
              the CLI the studio spawns
            </span>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {harnesses.map((h) => {
              const isActive = session?.harness === h.id;
              return (
                <button
                  key={h.id}
                  type="button"
                  className={`row ${isActive ? "is-active" : ""}`}
                  onClick={() => void setHarness(h.id)}
                  disabled={!h.available}
                  title={harnessHint(h)}
                >
                  <span
                    aria-hidden
                    className={h.available ? (isActive ? "text-ok" : "text-ink-faint") : "text-ink-faint"}
                    style={{ fontSize: 10 }}
                  >
                    ●
                  </span>
                  <span style={{ flex: 1, minWidth: 0 }} className="truncate">
                    {h.label}
                  </span>
                  <span className="list-card__row-meta">
                    {h.available ? (h.version ?? "installed") : "not installed"}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </section>

      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Appearance</h2>
          <div className="entry-section__actions">
            <span style={{ color: "var(--text-faint)", fontSize: 11 }}>theme</span>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {(["light", "dark"] as const).map((t) => {
              const isActive = config?.theme === t;
              return (
                <button
                  key={t}
                  type="button"
                  className={`row ${isActive ? "is-active" : ""}`}
                  onClick={() => void setTheme(t)}
                >
                  <span aria-hidden style={{ fontSize: 12, width: 16, textAlign: "center" }}>
                    {t === "light" ? "☀" : "☾"}
                  </span>
                  <span style={{ flex: 1, minWidth: 0, textAlign: "left" }}>
                    {t === "light" ? "Light" : "Dark"}
                  </span>
                  {isActive && (
                    <span style={{ color: "var(--brand)", fontSize: 11 }}>● active</span>
                  )}
                </button>
              );
            })}
          </div>
        </div>

        <div className="list-card" style={{ marginTop: 12 }}>
          <div className="list-card__body">
            <div style={{ padding: "10px 14px 0", fontSize: 11, color: "var(--text-faint)" }}>
              density
            </div>
            {(["comfortable", "compact"] as const).map((d) => {
              const isActive = (config?.density ?? "comfortable") === d;
              return (
                <button
                  key={d}
                  type="button"
                  className={`row ${isActive ? "is-active" : ""}`}
                  onClick={() => void setDensity(d)}
                >
                  <span aria-hidden style={{ fontSize: 12, width: 16, textAlign: "center" }}>
                    {d === "compact" ? "≡" : "☰"}
                  </span>
                  <span style={{ flex: 1, minWidth: 0, textAlign: "left" }}>
                    {d === "compact" ? "Compact" : "Comfortable"}
                    {d === "compact" && (
                      <span style={{ color: "var(--text-faint)", marginLeft: 6, fontSize: 11 }}>
                        tighter spacing for laptops
                      </span>
                    )}
                  </span>
                  {isActive && (
                    <span style={{ color: "var(--brand)", fontSize: 11 }}>● active</span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      </section>

      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Privacy</h2>
          <div className="entry-section__actions">
            <span style={{ color: "var(--text-faint)", fontSize: 11 }}>
              anonymous usage telemetry
            </span>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {([false, true] as const).map((on) => {
              const isActive = config?.share_analytics === on;
              return (
                <button
                  key={String(on)}
                  type="button"
                  className={`row ${isActive ? "is-active" : ""}`}
                  onClick={() => void setShareAnalytics(on)}
                >
                  <span aria-hidden style={{ fontSize: 12, width: 16, textAlign: "center" }}>
                    {on ? "●" : "○"}
                  </span>
                  <span style={{ flex: 1, minWidth: 0, textAlign: "left" }}>
                    {on ? "Enabled" : "Disabled"}
                  </span>
                  {isActive && (
                    <span style={{ color: "var(--brand)", fontSize: 11 }}>● active</span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
        <p style={{ marginTop: 8, fontSize: 11, color: "var(--text-faint)", lineHeight: 1.6 }}>
          Off by default. When disabled, renders run with HyperFrames' telemetry
          disabled; enable it to share anonymous usage with the team.
        </p>
      </section>

      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">About</h2>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            <div className="list-card__row">
              <span className="list-card__row-title">Navya Studio</span>
              <span className="list-card__row-meta">v0.1.0</span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">Default agent</span>
              <span className="list-card__row-meta">{session?.harness ?? "—"}</span>
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}
