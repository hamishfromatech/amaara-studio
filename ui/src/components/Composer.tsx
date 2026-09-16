/**
 * Composer — the shared chat input (design.md §3 compose box).
 *
 * Used by both the Home hero and the dedicated ChatView so the draft,
 * attachments, and queued sends behave identically on either surface (the
 * draft lives in the store — switching surfaces never loses typed text).
 *
 * Anatomy:
 *   queued-sends strip   — prompts held while a run is in flight (FIFO)
 *   attachment chip row  — staged images/files (thumbnail + remove)
 *   textarea             — ⏎ send · ⇧⏎ newline · ⇧⌘↵ steer · ⌘⌥↵ follow-up
 *                          (IME-safe: composing Enter never sends)
 *   send bar             — Send / Steer / attach / hint
 *
 * Attachments: the webview has no fs access, so picked files are read as
 * base64 and the Rust core copies them into <project>/assets (the harness
 * spawns in the project dir). On send they are referenced by project-relative
 * path in the prompt text.
 */

import {useEffect, useRef} from 'react'
import {useStore} from '../lib/store'
import {formatBytes} from '../lib/format'

interface ComposerProps {
  /** Disable the whole composer (no project open). */
  disabled?: boolean
  placeholder?: string
  /** Focus the textarea on mount (ChatView does; Home lets pills drive it). */
  autoFocus?: boolean
  /** Compact variant for the ChatView (smaller textarea). */
  compact?: boolean
}

export function Composer({
  disabled = false,
  placeholder = 'Describe what you want to make.',
  autoFocus = false,
  compact = false,
}: ComposerProps) {
  const draft = useStore((s) => s.draft)
  const setDraft = useStore((s) => s.setDraft)
  const sendPrompt = useStore((s) => s.sendPrompt)
  const steer = useStore((s) => s.steer)
  const queuedPrompts = useStore((s) => s.queuedPrompts)
  const removeQueuedPrompt = useStore((s) => s.removeQueuedPrompt)
  const sendQueuedNow = useStore((s) => s.sendQueuedNow)
  const attachments = useStore((s) => s.attachments)
  const attachFiles = useStore((s) => s.attachFiles)
  const removeAttachment = useStore((s) => s.removeAttachment)

  const taRef = useRef<HTMLTextAreaElement | null>(null)
  const fileRef = useRef<HTMLInputElement | null>(null)

  // Auto-grow with the draft (capped by CSS max-height, then scroll).
  useEffect(() => {
    const ta = taRef.current
    if (!ta) return
    ta.style.height = 'auto'
    ta.style.height = `${ta.scrollHeight}px`
  }, [draft])

  const send = (mode: 'normal' | 'steer' | 'follow_up') => {
    const msg = draft.trim()
    if (!msg || disabled) return
    if (mode === 'steer') {
      void steer(msg)
    } else {
      void sendPrompt(msg, mode === 'follow_up' ? 'follow_up' : 'normal')
    }
    setDraft('')
  }

  return (
    <div className={`composer ${compact ? 'composer--compact' : ''}`}>
      {/* Queued sends (open-design QueuedSendStrip): prompts typed while a
          run is in flight — visible, removable, send-now, FIFO on turn end. */}
      {queuedPrompts.length > 0 && (
        <div className="queued-sends" role="list" aria-label="Queued prompts">
          {queuedPrompts.map((q, i) => (
            <span key={`${i}-${q.text.slice(0, 8)}`} className="queued-sends__item" role="listitem">
              {i === 0 && (
                <span className="queued-sends__next" title="sends when the current turn ends">
                  ● next
                </span>
              )}
              {q.mode === 'follow_up' && (
                <span className="queued-sends__mode" title="follow-up">
                  ⏎
                </span>
              )}
              <span className="queued-sends__text" title={q.text}>
                {q.text}
              </span>
              <button
                type="button"
                className="icon-btn"
                title="Send now"
                aria-label="Send now"
                onClick={() => sendQueuedNow(i)}
              >
                ▶
              </button>
              <button
                type="button"
                className="icon-btn"
                title="Remove"
                aria-label="Remove queued prompt"
                onClick={() => removeQueuedPrompt(i)}
              >
                ×
              </button>
            </span>
          ))}
        </div>
      )}

      {/* Staged attachments — images show a thumbnail, files a name chip. */}
      {attachments.length > 0 && (
        <div className="composer__attachments" role="list" aria-label="Attachments">
          {attachments.map((a) => (
            <span
              key={a.info.id}
              className={`composer__attach ${a.info.kind === 'image' ? 'composer__attach--image' : ''}`}
              role="listitem"
              title={a.info.path}
            >
              {a.previewUrl ? (
                <img src={a.previewUrl} alt={a.info.name} className="composer__attach-thumb" />
              ) : (
                <span className="composer__attach-glyph" aria-hidden>
                  ▤
                </span>
              )}
              <span className="composer__attach-name">{a.info.name}</span>
              <span className="composer__attach-size">{formatBytes(a.info.size_bytes)}</span>
              <button
                type="button"
                className="composer__attach-clear"
                title="Remove attachment"
                aria-label={`Remove attachment ${a.info.name}`}
                onClick={() => removeAttachment(a.info.id)}
              >
                ×
              </button>
            </span>
          ))}
        </div>
      )}

      <textarea
        ref={taRef}
        className="composer__textarea"
        placeholder={placeholder}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          // Enter sends, Shift+Enter is a newline, IME-safe (CJK input:
          // Enter confirming a candidate must never send).
          if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
            const mod = e.metaKey || e.ctrlKey
            if (!mod && !e.shiftKey) {
              e.preventDefault()
              send('normal')
              return
            }
            if (mod) {
              e.preventDefault()
              send(e.altKey ? 'follow_up' : e.shiftKey ? 'steer' : 'normal')
            }
          }
        }}
        rows={compact ? 2 : 3}
        disabled={disabled}
        autoFocus={autoFocus}
        aria-label="Message the agent"
      />

      <div className="composer__send-bar">
        <button
          className="btn btn-primary btn-sm"
          onClick={() => send('normal')}
          disabled={disabled || !draft.trim()}
          title="Send (⏎)"
        >
          Send
        </button>
        <button
          className="btn btn-ghost btn-sm"
          onClick={() => send('steer')}
          disabled={disabled || !draft.trim()}
          title="Steer the agent without breaking the current turn (⇧⌘↵)"
        >
          Steer
        </button>
        <button
          className="btn btn-ghost btn-sm"
          onClick={() => fileRef.current?.click()}
          disabled={disabled}
          title="Attach images or files (saved to the project assets dir)"
        >
          ⌇ Attach
        </button>
        <input
          ref={fileRef}
          type="file"
          multiple
          accept="image/*,.md,.txt,.html,.htm,.json,.csv,.ts,.tsx,.js,.mjs"
          hidden
          onChange={(e) => {
            if (e.target.files) void attachFiles(e.target.files)
            e.target.value = '' // allow re-picking the same file
          }}
        />
        <span className="composer__hint">⏎ send · ⇧⏎ newline · ⇧⌘↵ steer · ⌘⌥↵ follow-up</span>
      </div>
    </div>
  )
}
