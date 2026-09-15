/**
 * Amaara Studio Extension for a-coder-cli (Phase 5).
 *
 * Registers the studio tools via pi.registerTool and proxies each call to the
 * control server's HTTP API (127.0.0.1:<port>/tool/<name>) with bearer token auth.
 *
 * Contract (a-coder-cli docs/extensions.md Quick Start): the default export
 * must be a FUNCTION receiving the ExtensionAPI - `export default function (pi)`.
 * An object export stalls the CLI's extension loader (the RPC loop never
 * becomes ready - measured: 6.3s to answer without the extension, >150s with
 * the broken shape).
 *
 * Tool execute signature: async execute(toolCallId, params, signal, onUpdate, ctx)
 * returning { content: [{ type: "text", text }], details } - errors are
 * returned as text content so the model can react instead of crashing the turn.
 */

// Control server URL and token from environment (set by the harness adapter).
const CONTROL_URL = process.env.AMAARA_CONTROL_URL || "http://127.0.0.1:8080";
const CONTROL_TOKEN = process.env.AMAARA_CONTROL_TOKEN;

// Tool definitions matching crates/amaara-tools (the control server dispatch).
const TOOLS = [
  {
    name: "generate_image",
    description: "Generate an image via Amaara Cloud or local sd-server.",
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

/**
 * Execute a tool call via the control server HTTP API.
 */
async function executeTool(toolName: string, args: any) {
  if (!CONTROL_TOKEN) {
    throw new Error("AMAARA_CONTROL_TOKEN not set; ensure the harness adapter is active.");
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

export default function (pi: any) {
  for (const tool of TOOLS) {
    pi.registerTool({
      name: tool.name,
      label: tool.name,
      description: tool.description,
      parameters: tool.parameters,
      async execute(_toolCallId: string, params: any) {
        try {
          const result = await executeTool(tool.name, params);
          return {
            content: [{ type: "text", text: JSON.stringify(result) }],
            details: {},
          };
        } catch (err: any) {
          return {
            content: [{ type: "text", text: `Tool ${tool.name} failed: ${err?.message ?? String(err)}` }],
            details: {},
          };
        }
      },
    });
  }
}
