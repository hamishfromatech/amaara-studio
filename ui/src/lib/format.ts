/**
 * Tiny formatting helpers used by the post-onboarding surfaces.
 *
 * Kept dependency-free so they work in both the renderer and any test
 * harness without pulling in a date library.
 */

/** Relative timestamp like "just now", "5m ago", "3h ago", "yesterday",
 *  "Mar 12". Falls back to a locale date for very old timestamps. */
export function formatRelative(ms: number, now = Date.now()): string {
  const diff = Math.max(0, now - ms)
  const sec = Math.floor(diff / 1000)
  if (sec < 45) return 'just now'
  const min = Math.floor(sec / 60)
  if (min < 60) return `${min}m ago`
  const hr = Math.floor(min / 60)
  if (hr < 24) return `${hr}h ago`
  const day = Math.floor(hr / 24)
  if (day === 1) return 'yesterday'
  if (day < 7) return `${day}d ago`
  const d = new Date(ms)
  // Same year → short month + day; otherwise include year.
  const sameYear = d.getFullYear() === new Date(now).getFullYear()
  return d.toLocaleDateString(
    undefined,
    sameYear ? {month: 'short', day: 'numeric'} : {year: 'numeric', month: 'short', day: 'numeric'},
  )
}

/** Hash a string into an integer. Tiny FNV-1a — good enough for swatch
 *  selection where we only need stable distribution, not crypto strength. */
export function hashString(s: string): number {
  let h = 0x811c9dc5
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return h >>> 0
}

/** Deterministic, muted swatch color for project cards. We pick from a
 *  short palette of desaturated hues (slate / olive / sand / mauve /
 *  teal / dusk) so every project feels native to the neutral product
 *  surface — never a saturated rainbow. Returns a CSS color string. */
export function swatchFor(seed: string): string {
  // Deep, warm, low-saturation tints that sit well on both the paper and
  // darkroom themes — used as the "frame" behind the project initial.
  const palette = [
    '#232f22',
    '#2c2a21',
    '#21302c',
    '#252938',
    '#302631',
    '#1f3030',
    '#33291f',
    '#312524',
  ]
  return palette[hashString(seed) % palette.length]
}

/** Compact elapsed label: "42s", then "3m07s" (open-design's run clock). */
export function elapsedLabel(startedAtMs: number, nowMs: number): string {
  const s = Math.max(0, Math.floor((nowMs - startedAtMs) / 1000))
  if (s < 60) return `${s}s`
  return `${Math.floor(s / 60)}m${String(s % 60).padStart(2, '0')}s`
}

/** Compact byte size: "12 KB", "1.4 MB". */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return '0 B'
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10240 ? 1 : 0)} KB`
  return `${(n / (1024 * 1024)).toFixed(1)} MB`
}
