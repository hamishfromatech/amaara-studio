/**
 * Center-pane navigation ids.
 *
 * Kept in its own module so both the store (single source of truth for the
 * active tab) and the nav rail can share the type without a circular import:
 * EntryNavRail imports the store, and the store would otherwise import NavId
 * from EntryNavRail.
 */
export type NavId =
  | 'home'
  | 'chat'
  | 'projects'
  | 'timeline'
  | 'renders'
  | 'models'
  | 'sources'
  | 'tools'
  | 'settings'

/** Human label for a center pane (palette / shortcuts help). */
export const NAV_LABELS: Record<NavId, string> = {
  home: 'Home',
  chat: 'Chat',
  projects: 'Projects',
  timeline: 'Timeline',
  renders: 'Renders',
  models: 'Models',
  sources: 'Sources',
  tools: 'Tools',
  settings: 'Settings',
}
