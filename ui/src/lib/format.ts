/**
 * Tiny formatting helpers used by the post-onboarding surfaces.
 *
 * Kept dependency-free so they work in both the renderer and any test
 * harness without pulling in a date library.
 */

/** Relative timestamp like "just now", "5m ago", "3h ago", "yesterday",
 *  "Mar 12". Falls back to a locale date for very old timestamps. */
export function formatRelative(ms: number, now = Date.now()): string {
  const diff = Math.max(0, now - ms);
  const sec = Math.floor(diff / 1000);
  if (sec < 45) return "just now";
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  if (day === 1) return "yesterday";
  if (day < 7) return `${day}d ago`;
  const d = new Date(ms);
  // Same year → short month + day; otherwise include year.
  const sameYear = d.getFullYear() === new Date(now).getFullYear();
  return d.toLocaleDateString(undefined, sameYear
    ? { month: "short", day: "numeric" }
    : { year: "numeric", month: "short", day: "numeric" });
}

/** Hash a string into an integer. Tiny FNV-1a — good enough for swatch
 *  selection where we only need stable distribution, not crypto strength. */
export function hashString(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** Deterministic, muted swatch color for project cards. We pick from a
 *  short palette of desaturated hues (slate / olive / sand / mauve /
 *  teal / dusk) so every project feels native to the neutral product
 *  surface — never a saturated rainbow. Returns a CSS color string. */
export function swatchFor(seed: string): string {
  const palette = [
    "#d6d3cc", // warm gray
    "#c8c5b6", // sand
    "#bcc3b3", // sage
    "#b9c0c8", // slate
    "#c5bcc8", // mauve
    "#bbc8c5", // teal-gray
    "#d3c5b9", // dusk
    "#c8beb9", // rose-gray
  ];
  return palette[hashString(seed) % palette.length];
}
