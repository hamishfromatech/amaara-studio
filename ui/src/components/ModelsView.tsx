/**
 * ModelsView — model catalog grouped by source (Cloud / Local).
 *
 * Moved out of the previous content-heavy LeftRail. Uses the list-card
 * primitive with two stacked panels; each row is a selectable model with
 * a green active dot, mono source tag, and kind chip on the right.
 */

import { useStore } from "../lib/store";

export function ModelsView() {
  const models = useStore((s) => s.models) ?? [];
  const session = useStore((s) => s.session);
  const setModel = useStore((s) => s.setModel);

  // The active harness's models lead the picker: the prompt runs through
  // that harness, so its catalog is what "set model" controls (live RPC for
  // a-coder-cli, static catalog for the others).
  const harness = models.filter((m) => m.source === "harness");
  const cloud = models.filter((m) => m.source === "cloud");
  const local = models.filter((m) => m.source === "local");
  const engine = models.filter((m) => m.source === "engine");

  return (
    <div className="entry-main__scroll-inner entry-main__scroll-inner--narrow">
      {harness.length > 0 && (
        <section className="entry-section">
          <div className="entry-section__head">
            <h2 className="entry-section__title">
              Harness — {session?.harness ?? "agent"}
            </h2>
            <div className="entry-section__actions">
              <span style={{ color: "var(--text-faint)", fontFamily: "var(--mono)", fontSize: 10 }}>
                {harness.length} available
              </span>
            </div>
          </div>
          <div className="list-card">
            <div className="list-card__body">
              {harness.map((m) => (
                <button
                  key={m.id}
                  type="button"
                  className={`row ${m.active ? "is-active" : ""}`}
                  onClick={() => void setModel(m.id)}
                >
                  <span
                    aria-hidden
                    className={m.active ? "text-ok" : "text-ink-faint"}
                    style={{ fontSize: 10 }}
                  >
                    ●
                  </span>
                  <span style={{ flex: 1, minWidth: 0 }} className="truncate">{m.name}</span>
                  <span className="list-card__row-meta">{m.kind}</span>
                </button>
              ))}
            </div>
          </div>
        </section>
      )}

      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Cloud Models</h2>
          <div className="entry-section__actions">
            <span style={{ color: "var(--text-faint)", fontFamily: "var(--mono)", fontSize: 10 }}>
              {cloud.length} available
            </span>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {cloud.length === 0 && (
              <div style={{ padding: "14px 18px", color: "var(--text-faint)", fontSize: 12 }}>
                No cloud models loaded yet.
              </div>
            )}
            {cloud.map((m) => (
              <button
                key={m.id}
                type="button"
                className={`row ${m.active ? "is-active" : ""}`}
                onClick={() => void setModel(m.id)}
              >
                <span
                  aria-hidden
                  className={m.active ? "text-ok" : "text-ink-faint"}
                  style={{ fontSize: 10 }}
                >
                  ●
                </span>
                <span style={{ flex: 1, minWidth: 0 }} className="truncate">{m.name}</span>
                <span className="list-card__row-meta">{m.kind}</span>
              </button>
            ))}
          </div>
        </div>
      </section>

      {engine.length > 0 && (
        <section className="entry-section">
          <div className="entry-section__head">
            <h2 className="entry-section__title">Engine (local proxy)</h2>
            <div className="entry-section__actions">
              <span style={{ color: "var(--text-faint)", fontFamily: "var(--mono)", fontSize: 10 }}>
                {engine.length} available
              </span>
            </div>
          </div>
          <div className="list-card">
            <div className="list-card__body">
              {engine.map((m) => (
                <button
                  key={m.id}
                  type="button"
                  className={`row ${m.active ? "is-active" : ""}`}
                  onClick={() => void setModel(m.id)}
                >
                  <span
                    aria-hidden
                    className={m.active ? "text-ok" : "text-ink-faint"}
                    style={{ fontSize: 10 }}
                  >
                    ●
                  </span>
                  <span style={{ flex: 1, minWidth: 0 }} className="truncate">{m.name}</span>
                  <span className="list-card__row-meta">{m.kind}</span>
                </button>
              ))}
            </div>
          </div>
        </section>
      )}

      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Local Models</h2>
          <div className="entry-section__actions">
            <span style={{ color: "var(--text-faint)", fontFamily: "var(--mono)", fontSize: 10 }}>
              {local.length} available
            </span>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {local.length === 0 && engine.length === 0 && (
              <div style={{ padding: "14px 18px", color: "var(--text-faint)", fontSize: 12 }}>
                No local models detected. Start the SD sidecar or Navya Engine to register one.
              </div>
            )}
            {local.map((m) => (
              <button
                key={m.id}
                type="button"
                className={`row ${m.active ? "is-active" : ""}`}
                onClick={() => void setModel(m.id)}
              >
                <span
                  aria-hidden
                  className={m.active ? "text-ok" : "text-ink-faint"}
                  style={{ fontSize: 10 }}
                >
                  ●
                </span>
                <span style={{ flex: 1, minWidth: 0 }} className="truncate">{m.name}</span>
                <span className="list-card__row-meta">{m.kind}</span>
              </button>
            ))}
          </div>
        </div>
      </section>
    </div>
  );
}
