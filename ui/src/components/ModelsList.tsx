/**
 * Models List Component (Phase 9).
 *
 * UI ModelsList groups Cloud (Navya) vs Local (llama.cpp + sd-server), tags
 * image/video models, marks the active model with ✓.
 */

import React from "react";

export interface ModelInfo {
  id: string;
  name?: string;
  kind: "chat" | "image" | "video" | "audio";
  is_active?: boolean;
}

interface ModelsListProps {
  cloudModels: ModelInfo[];
  localModels: ModelInfo[];
  activeModelId: string;
  onModelSelect: (modelId: string) => void;
}

export function ModelsList({ cloudModels, localModels, activeModelId, onModelSelect }: ModelsListProps) {
  return (
    <div className="space-y-4">
      {/* Cloud (Navya) section */}
      <div>
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">Cloud (Navya)</h3>
        <div className="space-y-1">
          {cloudModels.map((model) => (
            <button
              key={model.id}
              onClick={() => onModelSelect(model.id)}
              className={`w-full text-left text-sm p-1 rounded hover:bg-studio-800 flex items-center justify-between ${
                model.is_active ? "text-accent font-medium" : "text-slate-300"
              }`}
            >
              <span>
                {model.is_active && <span className="mr-2">✓</span>}
                {model.id}
                {model.kind !== "chat" && (
                  <span className="ml-2 text-xs text-slate-500">({model.kind})</span>
                )}
              </span>
            </button>
          ))}
        </div>
      </div>

      {/* Local section */}
      <div>
        <h3 className="text-xs font-semibold tracking-widest text-slate-400 mb-2">Local</h3>
        <div className="space-y-1">
          {localModels.map((model) => (
            <button
              key={model.id}
              onClick={() => onModelSelect(model.id)}
              className={`w-full text-left text-sm p-1 rounded hover:bg-studio-800 flex items-center justify-between ${
                model.is_active ? "text-accent font-medium" : "text-slate-300"
              }`}
            >
              <span>
                {model.is_active && <span className="mr-2">✓</span>}
                {model.id}
                {model.kind !== "chat" && (
                  <span className="ml-2 text-xs text-slate-500">({model.kind})</span>
                )}
              </span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
