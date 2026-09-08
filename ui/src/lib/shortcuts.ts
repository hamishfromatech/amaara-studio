/**
 * Global keyboard shortcuts (Phase 16).
 *
 * A single `window` keydown listener owned by the shell. It maps the keys in
 * `design.md` §10 to store actions. Focus is respected: plain keys (Space, J,
 * K, L, `?`) are ignored while the user is typing in an input/textarea/select,
 * while command/ctrl combos (⌘K, ⌘N, …) fire regardless so the palette is
 * reachable even mid-typing. Composer send/steer (⌘Enter family) are handled
 * inside the composer itself, since they need the typed text.
 */

import {useEffect} from 'react'
import {useStore} from './store'
import {NAV_LABELS, type NavId} from './nav'
import type {Clip} from './invoke'

/** Center tabs reachable by ⌘1..⌘4 (design.md §10: chat/timeline/renders/
 * models). */
const TAB_KEYS: NavId[] = ['chat', 'timeline', 'renders', 'models']

function isTypingTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null
  if (!el) return false
  const tag = el.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT'
}

/** Build the same targeted instruction the Inspector's "edit in chat" button
 * sends, so ⌘E produces an equivalent message for the selected clip. */
function editInstructionFor(clip: Clip | null): string | null {
  if (!clip) return null
  const start = clip.start_s
  const end = clip.start_s + clip.duration_s
  const fmt = (s: number) => {
    if (!Number.isFinite(s) || s < 0) return '0:00'
    const m = Math.floor(s / 60)
    const sec = Math.floor(s % 60)
    return `${m}:${String(sec).padStart(2, '0')}`
  }
  return `Adjust clip "${clip.id}" (currently ${fmt(start)}–${fmt(end)}): `
}

export function useKeyboardShortcuts() {
  const navId = useStore((s) => s.navId)
  const setNavId = useStore((s) => s.setNavId)
  const toggleLeftRail = useStore((s) => s.toggleLeftRail)
  const toggleRightRail = useStore((s) => s.toggleRightRail)
  const setPaletteOpen = useStore((s) => s.setPaletteOpen)
  const setShortcutsOpen = useStore((s) => s.setShortcutsOpen)
  const setRenderDialogOpen = useStore((s) => s.setRenderDialogOpen)
  const abort = useStore((s) => s.abort)
  const render = useStore((s) => s.render)
  const newProject = useStore((s) => s.newProject)
  const shuttle = useStore((s) => s.shuttle)
  const togglePlay = useStore((s) => s.togglePlay)
  const timeline = useStore((s) => s.timeline)
  const selectedClipId = useStore((s) => s.selectedClipId)
  const renderDialogOpen = useStore((s) => s.renderDialogOpen)

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      // IME guard (open-design QuickSwitcher): CJK composition input sends
      // synthetic keydowns (keyCode 229) mid-conversion — never hijack those.
      if (e.isComposing || e.keyCode === 229) return
      const key = e.key
      const mod = e.metaKey || e.ctrlKey
      const typing = isTypingTarget(e.target)

      // Don't hijack ordinary typing of unmodified keys.
      if (typing && !mod) return

      // Command palette — reachable from anywhere, even mid-typing.
      if (mod && (key === 'k' || key === 'K')) {
        e.preventDefault()
        setPaletteOpen(!useStore.getState().paletteOpen)
        return
      }

      // Everything below is for non-typing focus.
      if (typing) return

      // Shortcuts help overlay (?).
      if (key === '?' && !mod) {
        e.preventDefault()
        setShortcutsOpen(!useStore.getState().shortcutsOpen)
        return
      }

      // Escape closes open overlays + the render… dialog.
      if (key === 'Escape') {
        setPaletteOpen(false)
        setShortcutsOpen(false)
        setRenderDialogOpen(false)
        return
      }

      // Timeline transport — only meaningful on the timeline tab.
      if (!mod && navId === 'timeline') {
        if (key === ' ') {
          e.preventDefault()
          togglePlay()
          return
        }
        if (key === 'j' || key === 'J') {
          e.preventDefault()
          shuttle(-8_000)
          return
        }
        if (key === 'k' || key === 'K') {
          e.preventDefault()
          togglePlay()
          return
        }
        if (key === 'l' || key === 'L') {
          e.preventDefault()
          shuttle(8_000)
          return
        }
      }

      if (!mod) return

      // Command/ctrl-modified shortcuts.
      const k = key.toLowerCase()
      switch (k) {
        case 'n': {
          e.preventDefault()
          void (async () => {
            const name = window.prompt('Project name')
            if (!name?.trim()) return
            const dir = window.prompt('Project directory (absolute path)', '.')
            if (dir == null || !dir.trim()) return
            try {
              await newProject(name.trim(), dir.trim())
            } catch {
              /* surfaces via the status strip */
            }
          })()
          break
        }
        case '.':
          e.preventDefault()
          void abort()
          break
        case 'r':
          e.preventDefault()
          // Render the current composition with the last-used defaults.
          void render('draft', 'local')
          break
        case '\u21e5': {
          // ⌘⇧R — choose quality + target before rendering.
          setRenderDialogOpen(true)
          break
        }
        case ',':
          e.preventDefault()
          setNavId('settings')
          break
        case 'e': {
          e.preventDefault()
          const clip = timeline?.clips.find((c) => c.id === selectedClipId) ?? null
          const instruction = editInstructionFor(clip)
          if (instruction) {
            // Edit-in-chat sends from the full Chat surface (which owns the
            // composer), not the timeline tab.
            setNavId('chat')
            void useStore.getState().setDraft(instruction)
            requestAnimationFrame(() => {
              const ta = document.querySelector<HTMLTextAreaElement>('.composer__textarea')
              ta?.focus()
            })
          }
          break
        }
        case '\\':
          e.preventDefault()
          toggleLeftRail()
          break
        case '/':
          e.preventDefault()
          toggleRightRail()
          break
        default:
          if (/^[1-4]$/.test(key)) {
            const idx = Number(key) - 1
            if (TAB_KEYS[idx]) {
              e.preventDefault()
              setNavId(TAB_KEYS[idx])
            }
          }
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [
    navId,
    setNavId,
    toggleLeftRail,
    toggleRightRail,
    setPaletteOpen,
    setShortcutsOpen,
    setRenderDialogOpen,
    abort,
    render,
    newProject,
    shuttle,
    togglePlay,
    timeline,
    selectedClipId,
    renderDialogOpen,
  ])
}
