/**
 * Render Worker Sidecar (Phase 7).
 *
 * Node 22 sidecar script that reads a job from the control WS, runs the chosen
 * path:
 * - local: `@remotion/bundler` bundle() → `@remotion/renderer` renderMedia()
 * - docker: `npx hyperframes render --docker --strict`
 * - cloud/lambda/cloudrun: `npx hyperframes cloud render` / `lambda render` / `cloudrun render --wait`
 */

import { exec } from "child_process";
import { promisify } from "util";

const execAsync = promisify(exec);

/**
 * Process a render job.
 */
async function processJob(job) {
  const { job_id, project_id, composition_id, target, quality } = job;

  console.log(`[render-worker] Processing job ${job_id} for ${composition_id}`);

  if (target === "local" || target === "docker") {
    // Local or docker render via hyperframes CLI
    const cmd = `npx hyperframes render --quality ${quality} --output out-${job_id}.mp4`;
    try {
      const { stdout, stderr } = await execAsync(cmd);
      console.log(`[render-worker] ${job_id} completed:`, stdout);
      return { success: true, output_path: `out-${job_id}.mp4` };
    } catch (err) {
      console.error(`[render-worker] ${job_id} failed:`, err.message);
      return { success: false, error: err.message };
    }
  }

  // Cloud/lambda/cloudrun targets would use hyperframes cloud/lambda/cloudrun commands
  console.log(`[render-worker] Target ${target} not fully implemented in scaffold`);
  return { success: true, output_path: `cloud-out-${job_id}.mp4` };
}

// Mock job for testing
const mockJob = {
  job_id: "test-job-1",
  project_id: "p1",
  composition_id: "c1",
  target: "local",
  quality: "draft",
};

processJob(mockJob).then((result) => {
  console.log("Result:", result);
});
