/**
 * Lightweight virtualization for variable-height rows (long chat sessions).
 *
 * No dependencies: rows outside the visible window are replaced by spacer
 * divs sized from measured (or estimated) heights. Measured heights are cached
 * in a ref, so scrolling is a cheap linear walk over a mostly-stable array.
 * Good enough for thousands of chat messages without a virtualizer library.
 */

import {useCallback, useLayoutEffect, useRef, useState, type MutableRefObject} from 'react'

const DEFAULT_ESTIMATE = 120
const OVERSCAN = 6

export interface VirtualRows {
  /** Attach to the scrolling container. */
  containerRef: MutableRefObject<HTMLDivElement | null>
  /** Attach to the scroll container's onScroll. */
  onScroll: () => void
  /** First row index to render. */
  start: number
  /** One past the last row index to render. */
  end: number
  /** Height of the top spacer (px). */
  topPad: number
  /** Height of the bottom spacer (px). */
  bottomPad: number
  /** Total content height (px) — drives the spacer container. */
  totalHeight: number
  /** Ref factory: attach to each rendered row wrapper to measure it. */
  measureRef: (index: number) => (el: HTMLDivElement | null) => void
  /** Scroll the container to the end (new-message follow). */
  scrollToBottom: (smooth?: boolean) => void
  /** True while the user is within `nearBottomPx` of the end. */
  isNearBottom: boolean
}

export function useVirtualRows(count: number, estimate = DEFAULT_ESTIMATE): VirtualRows {
  const containerRef = useRef<HTMLDivElement | null>(
    null,
  ) as MutableRefObject<HTMLDivElement | null>
  const heights = useRef<Map<number, number>>(new Map())
  const [scrollTop, setScrollTop] = useState(0)
  const [viewportH, setViewportH] = useState(0)
  // Bumped whenever a measured height changes (recomputes the window).
  const [, setVersion] = useState(0)

  // Track the viewport height (window resizes / rail toggles).
  useLayoutEffect(() => {
    const el = containerRef.current
    if (!el) return
    const ro = new ResizeObserver(() => setViewportH(el.clientHeight))
    ro.observe(el)
    setViewportH(el.clientHeight)
    return () => ro.disconnect()
  }, [])

  // Forget measurements beyond the current count (chat cleared).
  useLayoutEffect(() => {
    for (const i of heights.current.keys()) {
      if (i >= count) heights.current.delete(i)
    }
  }, [count])

  const heightOf = (i: number): number => heights.current.get(i) ?? estimate

  // Prefix walk: find the first row whose bottom edge is at/below scrollTop.
  let start = 0
  let acc = 0
  while (start < count && acc + heightOf(start) <= scrollTop) {
    acc += heightOf(start)
    start++
  }
  // A tall row may straddle the top edge — include the one above.
  start = Math.max(0, start - 1)

  let end = start
  let windowSum = 0
  for (let i = start; i < count; i++) {
    const h = heightOf(i)
    if (windowSum + h > viewportH + estimate) break
    windowSum += h
    end = i + 1
  }
  end = Math.min(count, end + OVERSCAN)

  // Recompute the window sum across the overscan rows so the total stays
  // exact (topPad + window + bottomPad must equal the full content height).
  let sumStartToEnd = 0
  for (let i = start; i < end; i++) sumStartToEnd += heightOf(i)

  let topPad = 0
  for (let i = 0; i < start; i++) topPad += heightOf(i)
  let bottomPad = 0
  for (let i = end; i < count; i++) bottomPad += heightOf(i)

  const totalHeight = topPad + sumStartToEnd + bottomPad

  const measureRef = useCallback(
    (index: number) => (el: HTMLDivElement | null) => {
      if (!el) return
      const h = el.offsetHeight
      if (h > 0 && heights.current.get(index) !== h) {
        heights.current.set(index, h)
        setVersion((v) => v + 1)
      }
    },
    [],
  )

  const onScroll = useCallback(() => {
    const el = containerRef.current
    if (el) setScrollTop(el.scrollTop)
  }, [])

  const scrollToBottom = useCallback((smooth = false) => {
    const el = containerRef.current
    if (!el) return
    el.scrollTo({top: el.scrollHeight, behavior: smooth ? 'smooth' : 'auto'})
  }, [])

  const el = containerRef.current
  const isNearBottom = !!el && el.scrollHeight - el.scrollTop - el.clientHeight < 80

  return {
    containerRef,
    onScroll,
    start,
    end,
    topPad,
    bottomPad,
    totalHeight,
    measureRef,
    scrollToBottom,
    isNearBottom,
  }
}
