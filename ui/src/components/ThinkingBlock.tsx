/**
 * ThinkingBlock — the harness reasoning stream (ThinkingDelta), collapsed
 * by default. A quiet mono disclosure; pulses "thinking" while live.
 */

import {useState} from 'react'

export function ThinkingBlock({text, live}: {text: string; live: boolean}) {
  const [open, setOpen] = useState(false)
  const label = live ? 'thinking' : 'thought process'
  return (
    <div className="thinking-block">
      <button
        type="button"
        className={`thinking-block__head ${live ? 'thinking-block__head--live' : ''}`}
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
      >
        <span aria-hidden>{open ? '▾' : '▸'}</span>
        <span className="thinking-block__label">{label}</span>
        {live && <span className="shimmer-text">…</span>}
      </button>
      {open && <pre className="thinking-block__body">{text}</pre>}
    </div>
  )
}
