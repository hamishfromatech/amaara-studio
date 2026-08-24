/**
 * Render Worker Sidecar (production wiring).
 *
 * A line-based JSONL stdio protocol so the Rust core can spawn this worker,
 * feed it render jobs, and receive typed progress events:
 *
 *   Rust → worker (stdin):  {"type":"render","job":{job_id,project_id,composition_id,target,quality,width,height,fps,project_dir}}
 *   worker → Rust (stdout): {"type":"progress","job_id":"...","stage":"...","frame":N,"total_frames":M}
 *                            {"type":"completed","job_id":"...","output_path":"..."}
 *                            {"type":"failed","job_id":"...","error":"..."}
 *                            {"type":"log","job_id":"...","message":"..."}
 *
 * Local/docker renders shell out to `npx hyperframes render`; cloud targets
 * use the hyperframes cloud/lambda/cloudrun subcommands. If Node/npx/hyperframes
 * are missing the job fails with a clear cause (the Rust side also pre-checks).
 */

import { spawn } from "child_process";
import { createInterface } from "readline";

// --- stdio JSONL framing ----------------------------------------------------

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

async function processJob(job) {
  const { job_id, target = "local", quality = "draft", project_dir } = job;
  send({ type: "log", job_id, message: `starting ${target}/${quality} render` });

  const args = ["hyperframes", "render", "--quality", quality];
  if (target === "docker") args.push("--docker", "--strict");
  if (target === "cloud") args.unshift("cloud");
  if (target === "lambda") args.unshift("lambda");
  if (target === "cloudrun") args.unshift("cloudrun", "--wait");

  const output = `renders/out-${job_id}.mp4`;
  args.push("--output", output);

  // Run via npx so we don't assume hyperframes is a global binary.
  const child = spawn("npx", args, {
    cwd: project_dir || process.cwd(),
    env: process.env,
    shell: process.platform === "win32",
    stdio: ["ignore", "pipe", "pipe"],
  });

  let totalFrames = 0;
  let lastFrame = 0;

  child.stdout.on("data", (chunk) => {
    const text = chunk.toString();
    // HyperFrames/Remotion print progress like "frame 120/1140" or "[render] 10%".
    const m = text.match(/frame\s+(\d+)\s*\/\s*(\d+)/i);
    if (m) {
      lastFrame = parseInt(m[1], 10);
      totalFrames = parseInt(m[2], 10);
      send({ type: "progress", job_id, stage: "rendering frames", frame: lastFrame, total_frames: totalFrames });
    } else {
      send({ type: "log", job_id, message: text.trim() });
    }
  });
  child.stderr.on("data", (chunk) => send({ type: "log", job_id, message: chunk.toString().trim() }));

  return new Promise((resolve) => {
    child.on("error", (err) => {
      send({ type: "failed", job_id, error: `failed to start npx hyperframes: ${err.message}` });
      resolve();
    });
    child.on("close", (code) => {
      if (code === 0) {
        send({ type: "progress", job_id, stage: "finalizing", frame: totalFrames || lastFrame, total_frames: totalFrames || null });
        send({ type: "completed", job_id, output_path: output });
      } else {
        send({ type: "failed", job_id, error: `hyperframes render exited with code ${code}` });
      }
      resolve();
    });
  });
}

// --- main stdio loop --------------------------------------------------------

const rl = createInterface({ input: process.stdin });
send({ type: "log", job_id: "worker", message: "render worker ready" });

for await (const line of rl) {
  const trimmed = line.trim();
  if (!trimmed) continue;
  let msg;
  try {
    msg = JSON.parse(trimmed);
  } catch {
    send({ type: "log", job_id: "worker", message: `ignoring non-JSON line: ${trimmed.slice(0, 80)}` });
    continue;
  }
  if (msg.type === "render") {
    processJob(msg.job).catch((err) =>
      send({ type: "failed", job_id: msg.job?.job_id ?? "unknown", error: String(err) })
    );
  } else if (msg.type === "ping") {
    send({ type: "pong" });
  }
}

send({ type: "log", job_id: "worker", message: "render worker stdin closed" });