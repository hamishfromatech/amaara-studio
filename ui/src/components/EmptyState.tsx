/**
 * EmptyState — quiet placeholder for an empty surface (Phase 16 polish).
 *
 * Used where a panel has nothing to show yet (e.g. no projects). Keeps the
 * same restrained vocabulary as the rest of the studio: a mono glyph, a
 * muted title, an optional line of context, and an optional CTA slot. No
 * spinners hiding an absent state — the emptiness is the state.
 */

import type { ReactNode } from "react";

interface EmptyStateProps {
  /** Mono glyph/emoji shown above the title. */
  glyph?: string;
  /** Short heading for the empty surface. */
  title: string;
  /** Optional supporting line (truncated to ~34ch via CSS). */
  description?: string;
  /** Optional action slot (e.g. a "New project" button). */
  action?: ReactNode;
}

export function EmptyState({ glyph = "○", title, description, action }: EmptyStateProps) {
  return (
    <div className="empty-state">
      <span className="empty-state__glyph" aria-hidden>
        {glyph}
      </span>
      <div className="empty-state__title">{title}</div>
      {description && <div className="empty-state__desc">{description}</div>}
      {action && <div style={{ marginTop: 12 }}>{action}</div>}
    </div>
  );
}
