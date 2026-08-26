#!/usr/bin/env node
/**
 * Minimal `a-coder-cli --mode rpc` stub for the headless e2e test (Phase 15).
 *
 * Speaks the documented stdio-JSONL contract subset:
 *   - prompt      → agent_start, two text_delta message_updates, agent_end
 *   - get_available_models → response with /data/models
 *   - anything else → success response ack
 *
 * Exits when stdin closes. No dependencies — runs on plain Node ≥ 18.
 */

let buf = "";

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

function handle(cmd) {
  if (cmd.type === "prompt") {
    send({ type: "agent_start", agentId: "stub" });
    const msg = "stub reply";
    const half = Math.ceil(msg.length / 2);
    send({
      type: "message_update",
      message: {},
      assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: msg.slice(0, half) },
    });
    send({
      type: "message_update",
      message: {},
      assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: msg.slice(half) },
    });
    send({
      type: "agent_end",
      messages: [{ role: "assistant", stopReason: "stop", content: msg }],
    });
    return;
  }
  if (cmd.type === "get_available_models") {
    send({
      type: "response",
      command: "get_available_models",
      success: true,
      data: {
        models: [
          { id: "stub/stub-model", name: "Stub Model", kind: "chat" },
        ],
      },
    });
    return;
  }
  send({ type: "response", command: cmd.type ?? "unknown", success: true });
}

process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buf += chunk;
  let idx;
  while ((idx = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, idx).trim();
    buf = buf.slice(idx + 1);
    if (!line) continue;
    try {
      handle(JSON.parse(line));
    } catch {
      send({ type: "response", command: "parse_error", success: false, error: "bad json" });
    }
  }
});
process.stdin.on("end", () => process.exit(0));