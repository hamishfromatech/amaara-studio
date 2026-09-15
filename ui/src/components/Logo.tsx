/**
 * Amaara brand mark — a hexagonal film-frame with a phosphor play-triangle.
 * Replaces the generic ⬢ glyph everywhere the wordmark appears. Sized via
 * the `size` prop; inherits accent treatment from CSS where needed.
 */
export function Logo({size = 20, dim = false}: {size?: number; dim?: boolean}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden
      style={{display: 'block', flexShrink: 0}}
    >
      {/* hexagon frame */}
      <path
        d="M12 1.8 20.5 6.65v10.7L12 22.2 3.5 17.35V6.65L12 1.8Z"
        stroke={dim ? 'currentColor' : 'var(--text-strong)'}
        strokeWidth="1.6"
        strokeLinejoin="round"
      />
      {/* play triangle */}
      <path
        d="M9.6 8.2v7.6l6.4-3.8-6.4-3.8Z"
        fill={dim ? 'currentColor' : 'var(--brand)'}
      />
    </svg>
  )
}