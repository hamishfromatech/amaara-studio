/**
 * ToolsView — sidecar health + MCP server configuration.
 *
 *   Sidecars — live status of the studio's subprocesses (render worker,
 *              preview server, sd-server), from the store.
 *   MCP — the built-in `amaara-studio-tools` server (amaara-mcp): where it
 *         lives, whether its runtime (uv) is installed, what it proxies to,
 *         and the tools it exposes. Below it, the user's *additional* MCP
 *         servers (config.mcp_servers): add / edit / toggle / remove,
 *         persisted with the rest of the config.
 */

import {useEffect, useState} from 'react'
import {useStore} from '../lib/store'
import {Commands, type McpServerConfig, type McpStatus} from '../lib/invoke'

function statusTone(s: string): string {
  if (s === 'running' || s === 'loaded' || s === 'ready') return 'text-ok'
  if (s === 'starting' || s === 'stopped') return 'text-info'
  if (s === 'exited' || s === 'failed') return 'text-danger'
  return 'text-ink-faint'
}

/** The tools the built-in amaara-mcp server exposes (amaara_mcp/server.py). */
const BUILTIN_TOOLS = [
  {id: 'generate_image', kind: 'image'},
  {id: 'render_to_video', kind: 'video'},
  {id: 'list_local_models', kind: 'meta'},
  {id: 'set_generation_source', kind: 'meta'},
  {id: 'get_project_state', kind: 'meta'},
  {id: 'snapshot', kind: 'image'},
  {id: 'open_in_folder', kind: 'meta'},
]

const EMPTY_SERVER: McpServerConfig = {name: '', command: '', args: [], env: [], enabled: true}

export function ToolsView() {
  const sidecars = useStore((s) => s.sidecars) ?? []
  const refreshSidecars = useStore((s) => s.refreshSidecars)
  const config = useStore((s) => s.config)
  const saveConfig = useStore((s) => s.saveConfig)

  const [mcp, setMcp] = useState<McpStatus | null>(null)
  const [servers, setServers] = useState<McpServerConfig[]>([])
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [editingIdx, setEditingIdx] = useState<number | null>(null)
  const [adding, setAdding] = useState(false)

  const refresh = async () => {
    try {
      const status = await Commands.getMcpStatus()
      setMcp(status)
      setServers(status.mcp_servers)
      setDirty(false)
    } catch {
      /* status strip surfaces errors */
    }
  }
  useEffect(() => {
    void refresh()
  }, [])

  const mutate = (next: McpServerConfig[]) => {
    setServers(next)
    setDirty(true)
  }

  const save = async () => {
    if (!config) return
    setSaving(true)
    try {
      await saveConfig({...config, mcp_servers: servers})
      await refresh()
    } finally {
      setSaving(false)
    }
  }

  const updateServer = (idx: number, patch: Partial<McpServerConfig>) => {
    mutate(servers.map((s, i) => (i === idx ? {...s, ...patch} : s)))
  }

  const removeServer = (idx: number) => {
    mutate(servers.filter((_, i) => i !== idx))
    if (editingIdx === idx) setEditingIdx(null)
  }

  return (
    <div className="entry-main__scroll-inner">
      {/* Sidecars */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">Sidecars</h2>
          <div className="entry-section__actions">
            <button
              type="button"
              className="entry-section__action"
              title="Re-query sidecar status from the Rust core"
              onClick={() => void refreshSidecars()}
            >
              Refresh →
            </button>
            <span style={{color: 'var(--text-faint)', fontFamily: 'var(--mono)', fontSize: 10}}>
              {sidecars.length} registered
            </span>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            {sidecars.length === 0 && (
              <div style={{padding: '14px 18px', color: 'var(--text-faint)', fontSize: 12}}>
                No sidecars registered yet.
              </div>
            )}
            {sidecars.map((sc) => (
              <div key={sc.name} className="list-card__row">
                <span aria-hidden className={statusTone(sc.status)} style={{fontSize: 10}}>
                  ●
                </span>
                <span className="list-card__row-title">{sc.name}</span>
                <span
                  className={`list-card__row-meta ${statusTone(sc.status)}`}
                  title={sc.detail ?? sc.status}
                >
                  {sc.status}
                </span>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Built-in MCP server */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">MCP — amaara-studio-tools (built-in)</h2>
          <div className="entry-section__actions">
            <button className="entry-section__action" onClick={() => void refresh()}>
              Refresh →
            </button>
          </div>
        </div>
        <div className="list-card">
          <div className="list-card__body">
            <div className="list-card__row">
              <span className="list-card__row-title">Server</span>
              <span
                className={`list-card__row-meta ${mcp?.server_exists ? 'text-ok' : 'text-danger'}`}
                title={mcp?.server_py}
              >
                {mcp?.server_exists ? '● present' : '● missing'}
                {mcp?.server_py ? ` · ${mcp.server_py.split(/[\\/]/).pop()}` : ''}
              </span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">Workspace</span>
              <span className="list-card__row-meta" title={mcp?.mcp_dir}>
                {mcp?.mcp_dir ?? '—'}
              </span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">Runtime (uv)</span>
              <span
                className={`list-card__row-meta ${mcp?.uv_available ? 'text-ok' : 'text-danger'}`}
              >
                {mcp?.uv_available
                  ? `● ${mcp.uv_version ?? 'installed'}`
                  : '● not on PATH — install uv'}
              </span>
            </div>
            <div className="list-card__row">
              <span className="list-card__row-title">Proxies to</span>
              <span className="list-card__row-meta">
                {mcp?.control_url ?? '—'} {mcp?.has_control_token ? '· token set' : '· no token'}
              </span>
            </div>
          </div>
        </div>
        <div className="list-card" style={{marginTop: 12}}>
          <div className="list-card__head">
            <span className="list-card__head-title">Exposed tools</span>
            <span style={{fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-faint)'}}>
              stdio → MCP-capable harnesses
            </span>
          </div>
          <div className="list-card__body">
            {BUILTIN_TOOLS.map((tool) => (
              <div key={tool.id} className="list-card__row">
                <span aria-hidden style={{fontSize: 10, color: 'var(--text-faint)'}}>
                  ⌬
                </span>
                <span className="list-card__row-title" style={{fontFamily: 'var(--mono)'}}>
                  {tool.id}
                </span>
                <span className="list-card__row-meta">{tool.kind}</span>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Additional MCP servers (user-configured) */}
      <section className="entry-section">
        <div className="entry-section__head">
          <h2 className="entry-section__title">MCP — additional servers</h2>
          <div className="entry-section__actions">
            {dirty && (
              <>
                <button className="entry-section__action" onClick={() => void refresh()}>
                  Discard
                </button>
                <button
                  className="btn btn-primary btn-sm"
                  disabled={saving}
                  onClick={() => void save()}
                >
                  {saving ? 'Saving…' : 'Save changes'}
                </button>
              </>
            )}
            {!dirty && (
              <button className="entry-section__action" onClick={() => setAdding(true)}>
                + Add server
              </button>
            )}
          </div>
        </div>

        <div className="list-card">
          <div className="list-card__body">
            {servers.length === 0 && !adding && (
              <div style={{padding: '14px 18px', color: 'var(--text-faint)', fontSize: 12}}>
                No additional MCP servers. The built-in amaara-studio-tools server is always
                available to MCP-capable harnesses.
              </div>
            )}
            {servers.map((srv, idx) => (
              <div
                key={`${srv.name}-${idx}`}
                className="list-card__row"
                style={{alignItems: 'flex-start'}}
              >
                <input
                  type="checkbox"
                  checked={srv.enabled}
                  onChange={(e) => updateServer(idx, {enabled: e.target.checked})}
                  title={srv.enabled ? 'Enabled' : 'Disabled'}
                  style={{marginTop: 6}}
                />
                {editingIdx === idx ? (
                  <McpServerForm
                    value={srv}
                    onChange={(patch) => updateServer(idx, patch)}
                    onCancel={() => setEditingIdx(null)}
                  />
                ) : (
                  <>
                    <span
                      className="list-card__row-title"
                      title={srv.command ? `${srv.command} ${srv.args.join(' ')}` : undefined}
                    >
                      {srv.name || '(unnamed)'}
                      <span className="list-card__row-meta" style={{marginLeft: 8}}>
                        {srv.command} {srv.args.join(' ')}
                      </span>
                    </span>
                    <div style={{display: 'flex', gap: 4}}>
                      <button
                        className="icon-btn h-6 w-6"
                        title="Edit"
                        aria-label={`Edit MCP server ${srv.name}`}
                        onClick={() => setEditingIdx(idx)}
                      >
                        ✎
                      </button>
                      <button
                        className="icon-btn h-6 w-6"
                        title="Remove"
                        aria-label={`Remove MCP server ${srv.name}`}
                        onClick={() => removeServer(idx)}
                      >
                        ×
                      </button>
                    </div>
                  </>
                )}
              </div>
            ))}
            {adding && (
              <div className="list-card__row" style={{alignItems: 'flex-start'}}>
                <input type="checkbox" checked style={{marginTop: 6, opacity: 0.5}} disabled />
                <McpServerForm
                  value={EMPTY_SERVER}
                  onChange={(patch) => mutate([...servers, {...EMPTY_SERVER, ...patch}])}
                  onCancel={() => {
                    setAdding(false)
                    mutate(servers.slice(0, -1))
                  }}
                />
              </div>
            )}
          </div>
        </div>
        {adding && (
          <div style={{marginTop: 8, display: 'flex', gap: 6}}>
            <button
              className="btn btn-primary btn-sm"
              disabled={
                !servers[servers.length - 1]?.name.trim() ||
                !servers[servers.length - 1]?.command.trim()
              }
              onClick={() => {
                setAdding(false)
              }}
            >
              Add
            </button>
            <button
              className="btn btn-ghost btn-sm"
              onClick={() => {
                setAdding(false)
                mutate(servers.slice(0, -1))
              }}
            >
              Cancel
            </button>
          </div>
        )}
        <p style={{marginTop: 8, fontSize: 11, color: 'var(--text-faint)', lineHeight: 1.6}}>
          Additional servers are stored in the studio config and advertised to MCP-capable
          harnesses. Env rows are KEY=VALUE pairs, one per line.
        </p>
      </section>
    </div>
  )
}

/** Inline editor for one MCP server entry (name, command, args, env). */
function McpServerForm({
  value,
  onChange,
  onCancel,
}: {
  value: McpServerConfig
  onChange: (patch: Partial<McpServerConfig>) => void
  onCancel: () => void
}) {
  const envText = value.env.map(([k, v]) => `${k}=${v}`).join('\n')
  return (
    <div style={{flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 6}}>
      <div style={{display: 'flex', gap: 6}}>
        <input
          className="input input-sm"
          style={{width: 160}}
          placeholder="name"
          value={value.name}
          onChange={(e) => onChange({name: e.target.value})}
          aria-label="MCP server name"
        />
        <input
          className="input input-sm"
          style={{width: 120}}
          placeholder="command (uv, npx…)"
          value={value.command}
          onChange={(e) => onChange({command: e.target.value})}
          aria-label="MCP server command"
        />
        <button className="btn btn-ghost btn-sm" onClick={onCancel}>
          Cancel
        </button>
      </div>
      <input
        className="input input-sm"
        placeholder="args (space-separated)"
        value={value.args.join(' ')}
        onChange={(e) =>
          onChange({
            args: e.target.value
              .split(' ')
              .map((s) => s.trim())
              .filter(Boolean),
          })
        }
        aria-label="MCP server arguments"
      />
      <textarea
        className="input input-sm mono"
        rows={2}
        placeholder={'AMAARA_CONTROL_URL=http://…\nANOTHER_KEY=value'}
        value={envText}
        onChange={(e) => {
          const env: [string, string][] = e.target.value
            .split('\n')
            .map((l) => l.trim())
            .filter(Boolean)
            .map((l) => {
              const i = l.indexOf('=')
              return i > 0
                ? ([l.slice(0, i), l.slice(i + 1)] as [string, string])
                : ([l, ''] as [string, string])
            })
          onChange({env})
        }}
        aria-label="MCP server environment variables"
      />
    </div>
  )
}
