/**
 * Models List Component (Phase 9).
 *
 * UI ModelsList groups Cloud (Navya) vs Local (llama.cpp + sd-server), tags
 * image/video models, marks the active model with ✓.
 */

import React from 'react'

export interface ModelInfo {
  id: string
  name?: string
  kind: 'chat' | 'image' | 'video' | 'audio'
  is_active?: boolean
}

interface ModelsListProps {
  cloudModels: ModelInfo[]
  localModels: ModelInfo[]
  activeModelId: string
  onModelSelect: (modelId: string) => void
}

export function ModelsList({
  cloudModels,
  localModels,
  activeModelId,
  onModelSelect,
}: ModelsListProps) {
  const renderRow = (model: ModelInfo) => (
    <button
      key={model.id}
      onClick={() => onModelSelect(model.id)}
      className={`row justify-between ${model.is_active ? 'is-active' : ''}`}
    >
      <span className="flex min-w-0 items-center gap-2">
        {model.is_active && <span className="text-xs text-ok">✓</span>}
        <span className="truncate">{model.id}</span>
        {model.kind !== 'chat' && <span className="text-xs text-ink-faint">({model.kind})</span>}
      </span>
    </button>
  )

  return (
    <div className="space-y-5">
      {/* Cloud (Navya) section */}
      <div>
        <h3 className="rail-label">Cloud (Navya)</h3>
        <div className="space-y-0.5">{cloudModels.map(renderRow)}</div>
      </div>

      {/* Local section */}
      <div>
        <h3 className="rail-label">Local</h3>
        <div className="space-y-0.5">{localModels.map(renderRow)}</div>
      </div>
    </div>
  )
}
