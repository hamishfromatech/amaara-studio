/**
 * Navya Studio Extension for a-coder-cli (Phase 5).
 *
 * Registers the studio tools via pi.registerTool and proxies each call to the
 * control server's HTTP API (127.0.0.1:<port>/tool/<name>) with bearer token auth.
 */

import { createRequire } from "module";
const require = createRequire(import.meta.url);

// Tool definitions matching crates/navya-tools/lib.rs
const TOOLS = [
  {
    name: "generate_image",
    description: "Generate an image via Navya Cloud or local sd-server.",
    parameters: {
      type: "object",
      properties: {
        project_id: { type: "string" },
        composition_id: { type: "string", nullable: true },
        prompt: { type: "string" },
        model: { type: "string", nullable: true },
        size: { type: "string", nullable: true },
      },
      required: ["project_id", "prompt"],
    },
  },
  {
    name: "render_to_video",
    description: "Queue a video render via the HyperFrames render sidecar.",
    parameters: {
      type: "object",
      properties: {
        project_id: { type: "string" },
        composition_id: { type: "string" },
        quality: { type: "string", enum: ["draft", "high"] },
        target: { type: "string", enum: ["local", "docker", "cloud", "lambda", "cloudrun"] },
      },
      required: ["project_id", "composition_id", "quality", "target"],
    },
  },
  {
    name: "list_local_models",
    description: "List available cloud and local models.",
    parameters: { type: "object", properties: {} },
  },
  {
    name: "set_generation_source",
    description: "Set the generation source (cloud vs local).",
    parameters: {
      type: "object",
      properties: { source: { type: "string", enum: ["cloud", "local"] } },
      required: ["source"],
    },
  },
  {
    name: "get_project_state",
    description: "Get current project state from the store.",
    parameters: { type: "object", properties: {} },
  },
  {
    name: "snapshot",
    description: "Snapshot a frame at timecode t (ms) from the current composition.",
    parameters: {
      type: "object",
      properties: { timecode_ms: { type: "number" } },
      required: ["timecode_ms"],
    },
  },
];

// Control server URL and token from environment (set by harness adapter).
const CONTROL_URL = process.env.NAVYA_CONTROL_URL || "http://127.0.0.1:8080";
const CONTROL_TOKEN = process.env.NAVYA_CONTROL_TOKEN;

/**
 * Execute a tool call via the control server HTTP API.
 */
async function executeTool(toolName: string, args: any) {
  if (!CONTROL_TOKEN) {
    throw new Error("NAVYA_CONTROL_TOKEN not set; ensure harness adapter is active.");
  }

  const url = `${CONTROL_URL}/tool/${toolName}`;
  const res = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": `Bearer ${CONTROL_TOKEN}`,
    },
    body: JSON.stringify(args),
  });

  if (!res.ok) {
    const errText = await res.text().catch(() => "");
    throw new Error(`Tool ${toolName} failed: ${res.status} ${errText}`);
  }

  return res.json();
}

// Register tools with the a-coder-cli pi extension API.
export function registerTools(pi: any) {
  for (const tool of TOOLS) {
    const execFn = async (args: any) => {
      try {
        const result = await executeTool(tool.name, args);
        return {
          success: true,
          data: result,
        };
      } catch (err: any) {
        return {
          success: false,
          error: err.message,
        };
      }
    };

    pi.registerTool({
      name: tool.name,
      description: tool.description,
      parameters: tool.parameters,
      execute: execFn,
    });
  }
}

export default { registerTools };
