# open-design lessons for Navya Studio

Findings from a survey of the sibling `open-design` codebase (local-first
design workspace: `apps/web` React frontend, `apps/daemon` Node backend).
Each item: what they do (with file refs), and the Navya situation it improves.
Companion to `docs/research/*.md`; Phase 15/16 polish and beyond.

---

## UI / UX (from `apps/web/src`)

### 1. Failure recovery cards that live on the message
`ChatPane.tsx` L1355–1760 + `runtime/amr-guidance.ts` (`resolveRunFailureUi`).
The failed run's error is persisted **on the assistant message**, not in
ephemeral state, so it survives reload. A per-error-code resolver returns copy +
a primary recovery action (authorize / retry / resume). Only the message owning
the canonical error card suppresses its inline pill, so history keeps pills on
older failures.
→ **Navya**: map `errors.rs` `ErrorCode`/`UserAction` to one canonical recovery
card in the chat; retry re-runs without re-typing the prompt.

### 2. Memoized message list: prop-allowlist + callback-ref
`AssistantMessage.tsx` L232–285 — `memo` with an explicit
`ASSISTANT_MESSAGE_COMPARED_PROPS` allowlist; interaction callbacks excluded and
routed through a stable `assistantCallbacksRef`, so streaming deltas re-render
only the streaming row.
→ **Navya**: copy this structure for chat rows; without it, per-delta re-renders
grow with session length.

### 3. QueuedSendStrip — queue prompts mid-run
`ChatPane.tsx` L4167–4430. Sends while a run streams are queued: strip above
composer with drag-reorder (before/after edge indicator), edit-in-place,
send-now, "+N more"; a send-latch ref prevents double-Enter double-enqueue.
→ **Navya**: queue "now add captions" while a render streams.

### 4. "Preparing → Working" shimmer + elapsed clock anchored to persisted start
`AssistantMessage.tsx` L865–876, L1618–1681. Before any content, the footer
shimmers "Preparing…"; content flips it to "Working" with a clock anchored to
the persisted run start (remounts never restart it). In-flight tool payloads
(`liveToolInput`) stream into a plain monospace panel — no async highlighting
per delta, never persisted.
→ **Navya**: replaces spinners in chat + renders queue (design.md §9
"no spinners hiding state").

### 5. Tool category registry + collapsing + one execution-record disclosure
`ToolCard.tsx`: `toolCategoryForName()` maps tool names to stable product
categories; consecutive same-name tools collapse into a pill ("Editing ×3,
Done"); all tool ops fold into one collapsible disclosure while produced-file
cards stay flat above it.
→ **Navya**: `render_clip → trim → caption → render` tool storms stay readable.

### 6. Token file structure (`styles/tokens.css`)
State tints as `{color, bg, border}` triples via `color-mix`; `--selected`
deliberately separate from `--accent` ("a primary CTA and a selected state can
coexist without competing"); locked 6-radius scale; named duration/cubic-bezier
aliases (`--dur-quick/--dur-enter/--dur-exit`); one `[data-theme="dark"]` block;
prose scale locked to 12–16px.
→ **Navya**: already close (has color-triples + durations); adopt the
selected≠accent separation and a documented radius lock.

### 7. Chat virtualization + scroll anchoring
`ChatPane.tsx` L3662+: custom px-based virtualizer — only above **80 messages**,
900px overscan, measured rows, `initialTailRows = 16`, `alwaysIncludeKey` pins
the streaming row. Dual-threshold bottom-follow: 80px auto-follow vs 120px jump
button; on send, the just-sent turn anchors to viewport top via a dynamic tail
spacer that freezes once the user scrolls.
→ **Navya**: adopt the thresholds + `alwaysIncludeKey` pattern; skip the rail
until sessions are long.

### 8. Media-surface composer state machine
`home-hero/media-surfaces.ts`: `buildHomeMediaComposer(surface)` — each surface
(image/video/audio) has typed `InputFieldSpec[]`, default inputs, and a query
template; chips are a pure data table with a discriminated-union action type.
Starter prompts **fill** the composer, never auto-send.
→ **Navya**: ready-made blueprint for pre-submit video parameters
(model/duration/aspect) on the Home composer.

### 9. Iframe keep-alive pool + lazy asset previews
`IframeKeepAlivePool.tsx`: LRU pool (default 5) that *parks* detached preview
iframes in a hidden host instead of destroying them; revision counters +
`useSyncExternalStore`. Assets render lazy per kind (`loading="lazy"`,
`preload="metadata"`, `?v=mtime` cache-busting).
→ **Navya**: the timeline preview iframe and snapshot thumbnails face the
identical remount cost on tab switches.

### 10. QuickSwitcher + first-run hint mechanics
`QuickSwitcher.tsx`: tiered fuzzy scoring (basename > prefix > substring >
path, cap 50), recents-first empty state, **pure `nextCursor` extracted for
unit tests**, `scrollIntoView({block:'nearest'})`, IME guard
(`e.nativeEvent.isComposing`) before intercepting keys. `FirstArtifactHint.tsx`:
one-time hint whose once-ever budget is spent on **dismiss, not show** (a
remount can't burn it), delayed-mount 600ms, re-measures its anchor at
400/1200/2600ms, deliberately non-modal.
→ **Navya**: harden the ⌘K palette with these; first-run hint anchored to the
Render button.

Smaller portables: Toast's ref-stabilized dismiss timer (prevents a ticking
counter from re-arming forever — a bug class Navya's render elapsed counters
will hit); `foldStrategyTaskTurns` (multi-run turns render as one); Todo card
hoisted above the composer; question-forms in-stream with localStorage drafts;
draft-seeding nonce (same text pushed into composer more than once).

---

## Backend / performance (from `apps/daemon/src`)

### B1. SSE single-write event assembly + unref'd keepalive
`server.ts` `createSseResponse()` (L2703): each event assembled into **one
`res.write`** so `event:`/`data:` land in one TCP chunk (partial-event bug for
chunk-readers otherwise); keepalive heartbeat `setInterval(...).unref()` with
close/finish cleanup; `X-Accel-Buffering: no`.
→ **Navya**: the WS bridge (`control.rs` → `studio://event`) should coalesce
per-event writes and heartbeat; batch TextDelta flushes on a ~50ms timer
(design.md already budgets this) instead of per-delta socket writes.

### B2. Equal-jitter exponential backoff retry policy
`run-retry-policy.ts`: pure `computeRetryBackoffMs(attemptIndex, category,
random = Math.random)` — 2× growth, 8s cap, **equal jitter** (half fixed + half
random) to desynchronize concurrent retries, rate-limit class gets a bigger
base; injectable `random` for deterministic tests.
→ **Navya**: render/harness retries currently restart-once only; adopt the
pure-function backoff (same-run transient vs native-session-continue) for the
render worker and Navya cloud calls.

### B3. SQLite pragmas + PRAGMA-driven column migration checks
`db.ts` L57: `journal_mode = WAL`, `foreign_keys = ON`, then migrations that
read `PRAGMA table_info(...)` to decide which `ALTER TABLE`s to run
(additive, idempotent). Single shared connection + per-file memoized instance.
→ **Navya**: verify `store/mod.rs` sets WAL + foreign_keys (rusqlite defaults
to journal mode delete!) and add a `table_info`-driven migration runner.

### B4. Refcounted file watchers
`project-watchers.ts`: first subscribe lazy-creates a chokidar watcher, last
unsubscribe closes it ("never hold descriptors for projects no UI is looking
at"); per-segment ignore tests; `awaitWriteFinish` stability threshold; symlink
rejection.
→ **Navya**: watch the open project dir for composition/asset changes instead
of re-reading the timeline on demand; refcount so switching projects closes
watchers.

### B5. Delta-stream parser state machines with dedup invariants
`runtimes/json-event-stream.ts`: per-parser state objects (cursor text,
open tool-use sets, **reasoning-suffix tracking** — Codex replays accumulated
thinking text on every event, so only the unseen suffix is re-emitted), plus
dedupe guards for duplicate tool uses.
→ **Navya**: aacoder.rs already dedupes by tool id; add suffix tracking if a
harness ever replays accumulated text (codex adapter exists).

### B6. Error envelope + code taxonomy
`server.ts`: `sendApiError(res, 500, 'INTERNAL_ERROR', …)` with stable
machine-readable codes; SSE consumers get typed failure events with
`retryable` flags. Failure categories (`rate_limit`, `transient`, …) drive the
retry policy and user copy in one place.
→ **Navya**: `errors.rs` already has `ErrorCode`/`UserAction`; carry the code
through command results → store → recovery card (finding 1) consistently.

### B7. Singletons with explicit close + hot-reload-safe swap
`db.ts` swaps the DB instance when the file changes (`dbInstance && dbFile === file`),
`closeDatabase()` nulls it. App config cache invalidated by mtime.
→ **Navya**: `ProjectStore` could memoize prepared statements per connection the
same way (currently per-call prepares).

## Ranked backend adoptions

1. **WAL + foreign_keys + idempotent migrations** in `store/mod.rs` (B3) —
   cheapest, highest-value; writer contention disappears with WAL.
2. **50ms coalescing timer for TextDelta/Progress events** across the
   `studio://event` bridge (B1) — design.md already budgets <50ms batches.
3. **Pure retry-backoff function with equal jitter** for render/cloud calls (B2).
4. **Persist render-queue state across restarts** — open-design persists job
   state in SQLite; Navya's queue dies with the process.
5. **Refcounted project file watchers** (B4) for live composition reloads.
6. **Error envelope with stable codes end-to-end** (B6) feeding finding 1.

## Adopted already / not applicable

**Adopted (2026-08-27 commits):**
- WAL + foreign_keys + NORMAL sync + busy timeout in `store/mod.rs` (B3)
- Pure retry-backoff with equal jitter — `src/retry.rs` (B2)
- 50ms TextDelta coalescing on the `studio://event` bridge (B1)
- QueuedSendStrip + failure recovery card + IME guards (findings 1, 3)
- Preparing→Working distinction + persisted elapsed clock (finding 4)
- **Retry wiring**: generate_image retries once on network/429/5xx; the
  render worker re-queues once after backoff when it exits without a
  completion event (B2 wired)
- **Render-queue persistence**: the renders table is the durable source of
  truth; startup hydrates + reconciles jobs left "running" by a crashed
  session (backend finding 5)

**Deferred (large or blocked):**
- Chat virtualization (>80-message threshold pattern) — sessions still short
- Iframe keep-alive pool — needs the timeline preview under real tab-switching
  load first
- Refcounted project file watchers (B4) — pairs with live composition reload
- Lexical rich composer — plain textarea is enough for v1
- Codex reasoning-suffix dedup — adapter exists but untested against live codex

**Not applicable:** SSE keepalive (in-app Tauri events, not HTTP SSE);
codex reasoning-suffix dedup (adapter exists but untested against live codex).