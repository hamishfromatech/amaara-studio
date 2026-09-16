/**
 * ChatView — the dedicated conversation surface (design.md §3).
 *
 * Unlike the Home hero (which floats the last few messages), this is the
 * full cockpit: every turn of the session, top to bottom, with each tool
 * call rendered as a live card (collapsed by default, expandable to args
 * and output). Long sessions are virtualized (lib/virtualize) so the DOM
 * stays small no matter how long the run.
 *
 *   turn header — state dot (◷ thinking · ▶ tool-calling · ⏸ waiting-user
 *                 · ✓ done · ✗ error) + harness + model + elapsed clock
 *   content     — streamed text
 *   tool cards  — 🛠 name + status; expand → args, live partial, result
 *   composer    — shared with Home (draft persists across surfaces)
 */

import {useEffect, useMemo, useRef, useState} from 'react'
import {useStore, type ChatMessage, type ToolCall} from '../lib/store'
import {useVirtualRows} from '../lib/virtualize'
import {elapsedLabel} from '../lib/format'
import {Composer} from './Composer'
import {EmptyState} from './EmptyState'
import {ThinkingBlock} from './ThinkingBlock'
import {Markdown} from './Markdown'

// ---------------------------------------------------------------------------
// Turn header
// ---------------------------------------------------------------------------

function StatusDot({status, waiting}: {status: ChatMessage['status']; waiting: boolean}) {
  if (waiting)
    return (
      <span className="chat-status chat-status--waiting" title="Waiting for your approval">
        ⏸
      </span>
    )
  switch (status) {
    case 'thinking':
      return (
        <span className="chat-status chat-status--thinking" title="Thinking">
          ◷
        </span>
      )
    case 'tool-calling':
      return (
        <span className="chat-status chat-status--working" title="Tool running">
          ▶
        </span>
      )
    case 'done':
      return (
        <span className="chat-status chat-status--done" title="Turn complete">
          ✓
        </span>
      )
    case 'error':
      return (
        <span className="chat-status chat-status--error" title="Turn failed">
          ✗
        </span>
      )
    default:
      return (
        <span className="chat-status chat-status--idle" title="Idle">
          ○
        </span>
      )
  }
}

// ---------------------------------------------------------------------------
// Tool card (design.md §3 — the heart of the chat)
// ---------------------------------------------------------------------------

const COLLAPSED_LINES = 3

/** Truncate to a few lines with a "N lines · expand" affordance. */
function CollapsedText({text}: {text: string}) {
  const [expanded, setExpanded] = useState(false)
  const lines = text.split('\n')
  const long = lines.length > COLLAPSED_LINES
  const shown = expanded || !long ? lines : lines.slice(0, COLLAPSED_LINES)
  return (
    <div className="tool-card__pre">
      <pre className="tool-card__code">{shown.join('\n')}</pre>
      {long && (
        <button
          type="button"
          className="tool-card__more"
          onClick={() => setExpanded((e) => !e)}
          aria-expanded={expanded}
        >
          {expanded ? 'collapse' : `${lines.length} lines · expand`}
        </button>
      )}
    </div>
  )
}

function ToolCard({tool}: {tool: ToolCall}) {
  const [open, setOpen] = useState(false)
  const argsText = useMemo(() => {
    if (tool.args == null) return null
    try {
      return JSON.stringify(tool.args, null, 2)
    } catch {
      return String(tool.args)
    }
  }, [tool.args])

  const output = tool.result ?? tool.partial
  const stateLabel = tool.done
    ? tool.isError
      ? 'failed'
      : 'done'
    : tool.partial
      ? 'streaming'
      : 'running'

  return (
    <div
      className={`tool-card ${tool.done ? (tool.isError ? 'tool-card--error' : 'tool-card--done') : 'tool-card--running'}`}
    >
      <button
        type="button"
        className="tool-card__head"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-label={`Tool ${tool.name}, ${stateLabel}${open ? ', expanded' : ', collapsed'}`}
      >
        <span className="tool-card__glyph" aria-hidden>
          //
        </span>
        <span className="tool-card__name">{tool.name}</span>
        <span className={`tool-card__state tool-card__state--${stateLabel}`}>
          {tool.done ? (tool.isError ? '✗' : '✓') : tool.partial ? '…' : '▶'} {stateLabel}
        </span>
        <span className="tool-card__chevron" aria-hidden>
          {open ? '▾' : '▸'}
        </span>
      </button>
      {open && (
        <div className="tool-card__body">
          {argsText && (
            <div className="tool-card__section">
              <div className="tool-card__label">args</div>
              <CollapsedText text={argsText} />
            </div>
          )}
          {tool.partial && !tool.done && (
            <div className="tool-card__section">
              <div className="tool-card__label">output (live)</div>
              <CollapsedText text={tool.partial} />
            </div>
          )}
          {tool.done && output != null && output !== '' && (
            <div className="tool-card__section">
              <div className="tool-card__label">{tool.isError ? 'error' : 'result'}</div>
              <CollapsedText text={output} />
            </div>
          )}
          {tool.done && (output == null || output === '') && (
            <div className="tool-card__empty">no output</div>
          )}
        </div>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Thinking disclosure — the harness reasoning stream, collapsed by default.
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

function CopyButton({text}: {text: string}) {
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return
    const t = setTimeout(() => setCopied(false), 1200)
    return () => clearTimeout(t)
  }, [copied])
  return (
    <button
      type="button"
      className="icon-btn"
      title={copied ? 'Copied' : 'Copy message'}
      aria-label="Copy message"
      onClick={() => {
        navigator.clipboard?.writeText(text).then(
          () => setCopied(true),
          () => setCopied(false),
        )
      }}
    >
      {copied ? '✓' : '⧉'}
    </button>
  )
}

function ChatMessageRow({
  msg,
  waiting,
  nowMs,
  harness,
  model,
  onRetry,
}: {
  msg: ChatMessage
  waiting: boolean
  nowMs: number
  harness: string
  model: string
  onRetry: (prompt: string) => void
}) {
  if (msg.role === 'you') {
    const modeLabel =
      msg.mode === 'steer' ? '⚡ steer' : msg.mode === 'follow_up' ? '⏎ follow-up' : null
    return (
      <div className="chat-msg chat-msg--you">
        <div className="chat-msg__head">
          <span className="chat-msg__who">You</span>
          {modeLabel && <span className="chat-msg__mode">{modeLabel}</span>}
        </div>
        <div className="chat-msg__body">{msg.content}</div>
      </div>
    )
  }
  if (msg.role === 'system') {
    return (
      <div className="chat-msg chat-msg--system">
        <Markdown text={msg.content} />
      </div>
    )
  }
  // Agent turn.
  const runLive = msg.status === 'thinking' || msg.status === 'tool-calling'
  return (
    <div className={`chat-msg chat-msg--agent ${msg.status === 'error' ? 'chat-msg--error' : ''}`}>
      <div className="chat-msg__head">
        <StatusDot status={msg.status} waiting={waiting} />
        <span className="chat-msg__who">
          Agent
          {harness ? ` · ${harness}` : ''}
          {model ? ` · ${model}` : ''}
        </span>
        {runLive && <span className="live-dot" aria-hidden style={{marginRight: -2}} />}
        {runLive && (
          <span className="chat-msg__clock">
            {msg.status === 'thinking' && !msg.content && msg.tools.length === 0
              ? 'preparing…'
              : `working ${msg.startedAtMs ? elapsedLabel(msg.startedAtMs, nowMs) : '…'}`}
          </span>
        )}
        {msg.status === 'done' && <span className="chat-msg__clock">done</span>}
        {msg.status === 'error' && <span className="chat-msg__clock">error</span>}
        {msg.status === 'done' && msg.content && <CopyButton text={msg.content} />}
      </div>
      {msg.thinking && <ThinkingBlock text={msg.thinking} live={runLive} />}
      {msg.content && (
        <div className="chat-msg__body">
          <Markdown text={msg.content} />
          {runLive && <span className="stream-caret">▍</span>}
        </div>
      )}
      {msg.tools.length > 0 && (
        <div className="chat-msg__tools">
          {msg.tools.map((t: ToolCall) => (
            <ToolCard key={t.id} tool={t} />
          ))}
        </div>
      )}
      {msg.status === 'error' && msg.failedPrompt && (
        <div className="chat-msg__recover">
          <button type="button" className="btn btn-sm" onClick={() => onRetry(msg.failedPrompt!)}>
            ↻ Retry
          </button>
        </div>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// ChatView
// ---------------------------------------------------------------------------

export function ChatView() {
  const chat = useStore((s) => s.chat)
  const session = useStore((s) => s.session)
  const hasProject = !!session?.current_project_id
  const pendingApproval = useStore((s) => s.pendingApproval)
  const sendPrompt = useStore((s) => s.sendPrompt)

  // 1s ticker for the in-flight run's elapsed clock (anchored to run start).
  const [nowMs, setNowMs] = useState(() => Date.now())
  const runActive = chat.some(
    (m) => m.role === 'agent' && (m.status === 'thinking' || m.status === 'tool-calling'),
  )
  useEffect(() => {
    if (!runActive) return
    setNowMs(Date.now())
    const t = setInterval(() => setNowMs(Date.now()), 1000)
    return () => clearInterval(t)
  }, [runActive])

  const v = useVirtualRows(chat.length)
  const stickRef = useRef(true)
  const listRef = useRef<HTMLDivElement | null>(null)

  // Follow the bottom while new content streams — unless the user scrolled up
  // to read history (then they take the wheel until they come back down).
  const handleScroll = () => {
    stickRef.current = v.isNearBottom
    v.onScroll()
  }
  useEffect(() => {
    if (stickRef.current) v.scrollToBottom()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chat])

  // Jump to the newest message on first mount.
  useEffect(() => {
    stickRef.current = true
    if (listRef.current) listRef.current.scrollTop = listRef.current.scrollHeight
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const waiting = pendingApproval != null

  if (chat.length === 0) {
    return (
      <div className="chat-view">
        <div className="chat-view__header">
          <h2 className="entry-section__title">Chat</h2>
          <div className="entry-section__actions">
            <span className="mono" style={{fontSize: 10, color: 'var(--text-faint)'}}>
              {session?.harness ?? '—'} · {session?.model ?? '—'}
            </span>
          </div>
        </div>
        <div className="chat-view__empty">
          <EmptyState
            glyph="◐"
            title="No conversation yet"
            description="Ask the agent to make something — it will run the intent interview, write the composition, and render it."
          />
        </div>
        <div className="chat-view__composer">
          <Composer
            autoFocus
            compact
            disabled={!hasProject}
            placeholder={
              hasProject
                ? 'Ask the agent to make something…'
                : 'Open a project to enable the agent.'
            }
          />
        </div>
      </div>
    )
  }

  const lastAgent = [...chat].reverse().find((m) => m.role === 'agent')
  const visible = chat.slice(v.start, v.end)

  return (
    <div className="chat-view">
      <div className="chat-view__header">
        <h2 className="entry-section__title">Chat</h2>
        <div className="entry-section__actions">
          <span className="mono" style={{fontSize: 10, color: 'var(--text-faint)'}}>
            {chat.length} message{chat.length === 1 ? '' : 's'} · {session?.harness ?? '—'} ·{' '}
            {session?.model ?? '—'}
          </span>
        </div>
      </div>

      <div
        ref={(el) => {
          listRef.current = el
          v.containerRef.current = el
        }}
        onScroll={handleScroll}
        className="chat-view__list"
        role="log"
        aria-label="Conversation with the agent"
      >
        <div style={{height: v.totalHeight, position: 'relative'}}>
          <div style={{position: 'absolute', top: 0, left: 0, right: 0, height: v.topPad}} />
          <div style={{position: 'absolute', top: v.topPad, left: 0, right: 0}}>
            {visible.map((m, i) => {
              const idx = v.start + i
              return (
                <div key={m.id} ref={v.measureRef(idx)} className="chat-view__row">
                  <ChatMessageRow
                    msg={m}
                    waiting={waiting && lastAgent != null && m.id === lastAgent.id}
                    nowMs={nowMs}
                    harness={session?.harness ?? ''}
                    model={session?.model ?? ''}
                    onRetry={(prompt) => void sendPrompt(prompt, 'normal')}
                  />
                </div>
              )
            })}
          </div>
          <div
            style={{
              position: 'absolute',
              top: v.topPad + (v.totalHeight - v.topPad - v.bottomPad),
              left: 0,
              right: 0,
              height: v.bottomPad,
            }}
          />
        </div>
      </div>

      <div className="chat-view__composer">
        <Composer
          compact
          disabled={!hasProject}
          placeholder={hasProject ? 'Reply to the agent…' : 'Open a project to enable the agent.'}
        />
      </div>
    </div>
  )
}
