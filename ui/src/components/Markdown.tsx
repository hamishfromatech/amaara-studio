/**
 * Markdown — full GFM rendering for agent chat content.
 *
 * The harness streams markdown; printing it raw loses headers, code fences,
 * lists, and tables. This wraps react-markdown + remark-gfm with the
 * studio's darkroom styling (`.md` class in index.css) and routes links
 * through the `open_external` command so a click never navigates the
 * webview away from the studio.
 *
 * Memoized on the text: rows re-render on unrelated state (clock ticks,
 * tool updates) and re-parsing a long document each time is wasteful.
 * During streaming the parse runs per coalesced delta (~20/s worst case)
 * which is fine for message-sized documents.
 */

import {memo} from 'react'
import ReactMarkdown, {type Components} from 'react-markdown'
import remarkGfm from 'remark-gfm'
import {Commands} from '../lib/invoke'

function openInBrowser(href: string | undefined) {
  if (!href) return
  if (href.startsWith('http://') || href.startsWith('https://')) {
    void Commands.openExternal(href).catch(() => {
      /* non-fatal — the click just does nothing */
    })
  }
}

/** Best-effort language tag for a block code fence. */
function codeLanguage(node: unknown): string | null {
  const cls = (node as {className?: string[] | undefined} | null)?.className
  if (!Array.isArray(cls)) return null
  const lang = cls.find((c) => c.startsWith('language-'))
  return lang ? lang.slice('language-'.length) : null
}

const components: Components = {
  a: ({node: _node, children, href, ...props}) => (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault()
        openInBrowser(href)
      }}
      {...props}
    >
      {children}
    </a>
  ),
  pre: ({children}) => <pre className="md__pre">{children}</pre>,
  code: ({node: _node, className, children, ...props}) => {
    const text = String(children ?? '')
    const isBlock = /language-/.test(className ?? '') || text.includes('\n')
    if (!isBlock) {
      return (
        <code className="md__inline-code" {...props}>
          {children}
        </code>
      )
    }
    const lang = codeLanguage({_type: undefined, className})
    return (
      <code className={className} data-lang={lang ?? undefined} {...props}>
        {children}
      </code>
    )
  },
  // Images: constrain + round via CSS; loading is lazy (agent may embed
  // generated assets by path/URL).
  img: (props) => <img loading="lazy" {...props} />,
}

export const Markdown = memo(function Markdown({text}: {text: string}) {
  return (
    <div className="md">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={components}
        // No raw HTML from the harness — markdown only.
      >
        {text}
      </ReactMarkdown>
    </div>
  )
})