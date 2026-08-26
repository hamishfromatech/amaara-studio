# Navya Studio — Local Image Generation with stable-diffusion.cpp

Research report for `sd-server` / `sd-cli` integration. Based on the
`stable-diffusion.cpp` project (`leejet/stable-diffusion.cpp`) and the build-gaps
questions in `plan/BUILD-GAPS.md` §D.22–D.24 + §N.63.

## Executive summary

- Use `sd-server` as a persistent HTTP sidecar. It exposes **three API families**:
  native async `/sdcpp/v1/...`, OpenAI-compatible `/v1/...`, and WebUI-compatible
  `/sdapi/v1/...`.
- For the studio, the best integration path is the **native async API**
  (`POST /sdcpp/v1/img_gen` + `GET /sdcpp/v1/jobs/{id}`) because it gives job IDs,
  cancellation, and deterministic progress.
- Default bind: `http://127.0.0.1:1234/`. Override with `--listen-ip` and
  `--listen-port`.
- The server loads one diffusion pipeline at startup. Switching models requires
  restarting `sd-server` (no hot-swap endpoint was found).
- Build flavors are **compile-time**: CUDA (`-DSD_CUDA=ON`), Vulkan
  (`-DSD_VULKAN=ON` / `-DGGML_VULKAN=ON`), CPU (default). The installer must
  detect the GPU and ship the matching binary.

## Binaries

| Binary | Role | When to use |
|---|---|---|
| `sd-server` | Persistent HTTP server | Studio local image generation |
| `sd-cli` | One-shot CLI | Fallback / diagnostics / metadata inspection |

On Windows the files are named `sd-server.exe` / `sd-cli.exe`.

## Starting sd-server

```bash
# Standalone diffusion model + VAE + text encoder
.\\bin\\Release\\sd-server.exe \
  --diffusion-model  ..\\models\\diffusion_models\\z_image_turbo_bf16.safetensors \
  --vae ..\\models\\vae\\ae.sft \
  --llm ..\\models\\text_encoders\\qwen_3_4b.safetensors \
  --diffusion-fa --offload-to-cpu -v --cfg-scale 1.0 \
  --listen-ip 127.0.0.1 --listen-port 1234
```

Useful flags (from server source):

| Flag | Meaning |
|---|---|
| `--diffusion-model <path>` | Main diffusion weights |
| `--vae <path>` | VAE weights |
| `--llm <path>` | Text encoder / LLM (e.g. Qwen3 4B for standalone pipelines) |
| `--clip_l`, `--clip_g`, `--t5xxl`, `--t5xxl_2` | Per-encoder weights for SDXL/SD3/Flux |
| `--cfg-scale <n>` | Default CFG scale |
| `--diffusion-fa` | Flash attention in diffusion model |
| `--offload-to-cpu` | Keep weights in RAM, load into VRAM on demand |
| `--listen-ip <ip>` | Bind IP (default `127.0.0.1`) |
| `--listen-port <port>` | Bind port (default `1234`) |
| `-v`, `--verbose` | Verbose logging |
| `--serve-html-path <path>` | Optional custom web UI |

A custom host/port is set with **two separate flags**:

```bash
sd-server --listen-ip 127.0.0.1 --listen-port 7860
```

## API families

The server exposes three compatibility layers:

1. **Native async** — `/sdcpp/v1/...` (recommended for the studio)
2. **OpenAI-compatible** — `/v1/images/generations`, `/v1/images/edits`, `/v1/models`
3. **AUTOMATIC1111 / WebUI-compatible** — `/sdapi/v1/txt2img`, `/sdapi/v1/img2img`, etc.

The native API is the most capable: it returns job IDs, supports cancellation,
and exposes `/sdcpp/v1/capabilities`.

## Native async API details

### `GET /sdcpp/v1/capabilities`

Returns a JSON object describing the loaded model, backend, supported output
formats, etc. Useful for the UI to show "local model ready".

### `POST /sdcpp/v1/img_gen`

Initiates an asynchronous image generation job.

**Request body** (excerpt of stable fields):

```json
{
  "prompt": "a futuristic cityscape",
  "negative_prompt": "",
  "width": 768,
  "height": 512,
  "steps": 30,
  "cfg_scale": 7.0,
  "seed": 12345,
  "batch_count": 1,
  "output_format": "png",
  "output_compression": 100
}
```

Additional fields available (from the `SDGenerationParams` schema):

| Field | Type | Notes |
|---|---|---|
| `sample_params.sample_steps` | int | Alias for `steps` |
| `sample_params.sample_method` | string | Euler, DPM++, etc. |
| `sample_params.scheduler` | string | Scheduler name |
| `sample_params.guidance.scale` | number | CFG |
| `vae_tiling_params.enabled` | bool | Tiled VAE decode |
| `hires.enabled` | bool | High-res fix |
| `hires.upscaler`, `hires.scale`, `hires.steps`, `hires.denoising_strength` | various | Upscale pass |
| `control_strength` | number | ControlNet strength |
| `ip_adapter_strength` | number | IP-Adapter strength |
| `auto_resize_ref_image` | bool | Reference image handling |
| `increase_ref_index` | bool | Reference image handling |

The full native schema is the same as `sd_cpp_extra_args`.

**Response (200):**

```json
{ "job_id": "job-12345abcde" }
```

### `GET /sdcpp/v1/jobs/{id}`

Poll for status/result.

```json
{
  "id": "job_01HTXYZABC",
  "kind": "img_gen",
  "status": "queued",
  "created": 1775401200,
  "started": null,
  "completed": null,
  "queue_position": 2,
  "result": null,
  "error": null
}
```

When completed:

```json
{
  "id": "job_01HTXYZABC",
  "kind": "img_gen",
  "status": "completed",
  "created": 1775401200,
  "started": 1775401205,
  "completed": 1775401210,
  "queue_position": 0,
  "result": {
    "images": [
      { "index": 0, "b64_json": "...base64..." }
    ]
  },
  "error": null
}
```

Image results are returned as **base64 strings** (`b64_json`). The studio must
save them to the project's `assets/img/` directory.

Errors:

| HTTP | Body | Meaning |
|---|---|---|
| 404 | `{"error":"job not found"}` | Unknown job ID |
| 410 | `{"error":"job expired"}` | Job cleaned up after completion/ttl |
| 409 | `{"error":"job queue state changed before cancellation"}` | Race on cancel |

### `POST /sdcpp/v1/jobs/{id}/cancel`

Cancel a queued or generating job.

### `POST /sdcpp/v1/vid_gen`

Native video generation endpoint (LTX/Wan-style). Not required for Navya Studio
v1.

## OpenAI-compatible API details

### `POST /v1/images/generations`

OpenAI-shaped request:

```json
{
  "model": "sd-cpp-local",
  "prompt": "a futuristic cityscape",
  "n": 1,
  "size": "1024x1024",
  "response_format": "b64_json"
}
```

Response:

```json
{
  "data": [
    { "b64_json": "...base64..." }
  ]
}
}
```

### `GET /v1/models`

```json
{
  "data": [
    { "id": "sd-cpp-local", "object": "model", "owned_by": "local" }
  ]
}
```

Note: `sd_cpp_extra_args` can be embedded inside the `prompt` string for the
OpenAI endpoint:

```text
a futuristic cityscape <sd_cpp_extra_args>{"sample_params":{"sample_steps":28}}</sd_cpp_extra_args>
```

## sd-cli one-shot commands

Useful as a fallback or for diagnostics.

### SD 1.5

```bash
.\\bin\\sd-cli -m ..\\models\\sd-v1-4.ckpt -p "a lovely cat"
```

### SDXL

```bash
.\\bin\\sd-cli -m ..\\models\\sd_xl_base_1.0.safetensors \
  --vae ..\\models\\sdxl_vae-fp16-fix.safetensors \
  -H 1024 -W 1024 -p "a lovely cat" -v
```

### SD3 / SD3.5

```bash
.\\bin\\sd-cli -m ..\\models\\sd3.5_large.safetensors \
  --clip_l ..\\models\\clip_l.safetensors \
  --clip_g ..\\models\\clip_g.safetensors \
  --t5xxl ..\\models\\t5xxl_fp16.safetensors \
  -H 1024 -W 1024 \
  -p 'a lovely cat holding a sign says \"Stable diffusion 3.5 Large\"' \
  --cfg-scale 4.5 --sampling-method euler -v --clip-on-cpu
```

### Flux

```bash
.\\bin\\sd-cli --diffusion-model ..\\models\\flux1-dev-q3_k.gguf \
  --vae ..\\models\\ae.sft \
  --clip_l ..\\models\\clip_l.safetensors \
  --t5xxl ..\\models\\t5xxl_fp16.safetensors \
  -p "a lovely cat holding a sign says 'flux.cpp'" \
  --cfg-scale 1.0 --sampling-method euler -v --clip-on-cpu
```

Common flags:

| Flag | Meaning |
|---|---|
| `-m <path>` | Main model (single-file SD/SDXL/SD3) |
| `--diffusion-model <path>` | Diffusion weights (Flux GGUF / standalone) |
| `--vae <path>` | VAE |
| `--clip_l`, `--clip_g`, `--t5xxl`, `--t5xxl_2` | Text encoders |
| `-H`, `-W` | Height / width in pixels |
| `--steps`, `--sampling-method` | Steps and sampler |
| `--cfg-scale` | CFG scale |
| `--seed` | Seed |
| `-o`, `--output` | Output path |
| `--clip-on-cpu` | Offload text encoders to CPU |
| `--vae-on-cpu` | Offload VAE to CPU |
| `--diffusion-fa` | Flash attention |
| `--offload-to-cpu` | Offload weights to RAM |
| `--lora-model-dir <dir>` | LoRA directory |

## Build flavors

stable-diffusion.cpp GPU backends are **compile-time**:

| Backend | CMake flag | Notes |
|---|---|---|
| CPU (default) | none | Works everywhere; slow |
| OpenBLAS | `-DGGML_OPENBLAS=ON` | CPU acceleration |
| CUDA | `-DSD_CUDA=ON` | NVIDIA; ≥ 4 GB VRAM recommended |
| Vulkan | `-DSD_VULKAN=ON` or `-DGGML_VULKAN=ON` | Cross-vendor GPU |
| HipBLAS | `-DSD_HIPBLAS=ON` | AMD ROCm |
| Metal | `-DSD_METAL=ON` | macOS Apple Silicon |
| RPC | `-DSD_RPC=ON` | Distributed offloading |

Studio implication: ship **three** `sd-server` binaries per platform
(`cpu`, `cuda`, `vulkan`) and pick at first-run / install time.

### Detection logic

1. Windows: use `wmic` or DXGI to detect NVIDIA dGPU → `cuda` flavor.
2. If no NVIDIA but Vulkan 1.2+ capable → `vulkan` flavor.
3. Else → `cpu` flavor.
4. Prefer the `cpu` binary if detection fails or the user opts in.

### Example build commands

```bash
# CPU
mkdir build && cd build
cmake ..
cmake --build . --config Release

# CUDA
mkdir build && cd build
cmake .. -DSD_CUDA=ON
cmake --build . --config Release

# Vulkan
mkdir build && cd build
cmake .. -DSD_VULKAN=ON
cmake --build . --config Release
```

## Model formats

| Format | Notes |
|---|---|
| `.ckpt` | PyTorch checkpoint |
| `.safetensors` | Recommended; safe serialized weights |
| `.gguf` | GGML quantized; used for Flux diffusion models |

## Recommended models and licensing

| Family | Recommendation | License | Notes |
|---|---|---|---|
| SD 1.5 | `stable-diffusion-v1-5` | OpenRAIL-M | Lightweight, 4 GB VRAM |
| SDXL | `stable-diffusion-xl-base-1.0` | OpenRAIL-M | Better quality, 6–8 GB VRAM |
| SD3 / SD3.5 | Stability AI official | Stability AI Community License | Read terms; some non-commercial clauses |
| Flux | `flux1-dev-q3_k.gguf` + `ae.sft` + encoders | Flux Dev non-commercial | **Non-commercial** unless you have an API/pro license |

Studio v1 recommendation:

- **Cloud-first default**. Local path is opt-in.
- For local, default to a permissively licensed model (SD 1.5 or SDXL) and
  require the user to download/place weights.
- Do **not** redistribute Flux weights or SD3 weights without legal review.
- The `harness-pack` prompts should tell the agent to respect the user's model
  directory and not download weights automatically.

## VRAM / CPU floor

| Model | Minimum VRAM | Recommended VRAM | CPU RAM fallback |
|---|---|---|---|
| SD 1.5 fp16 | ~3 GB | 4 GB | 8 GB |
| SDXL fp16 | ~5 GB | 6–8 GB | 16 GB |
| SD3 / SD3.5 fp16 | ~6 GB | 8 GB | 16 GB |
| Flux Q4_K_M | ~8 GB | 12 GB | 32 GB |

Use `--offload-to-cpu` to trade speed for lower VRAM.

## Progress reporting

`sd-server` does **not** appear to expose a streaming progress endpoint
(WebSocket/SSE) in the public API. Progress is by **polling** the job:

```bash
GET /sdcpp/v1/jobs/{id}
```

The response `status` cycles:

```
queued → generating → completed | failed | cancelled
```

For richer progress, parse server logs (`-v`) or extend the HTTP layer later.

## Error handling

Errors are returned as JSON with an `error` field. The job status may also be
`failed` with an `error_message` field.

Studio should map common errors to user actions:

| Error | User action |
|---|---|
| Model file not found | Open settings → pick model dir |
| CUDA out of memory | Switch to vulkan/cpu flavor or enable offload |
| Job failed / timeout | Retry or check sd-server logs |

## Integration checklist for Navya Studio

1. Bundle `sd-server-<triple>.exe` for `cpu`, `cuda`, `vulkan` in
   `src-tauri/binaries/` (or download on first run).
2. Add settings fields: `sd_server_binary`, `sd_models_dir`, `sd_backend`.
3. On first local image request, detect GPU → pick binary → spawn
   `sd-server --listen-ip 127.0.0.1 --listen-port <X>` with model paths from
   the configured models dir.
4. Health-check via `GET /sdcpp/v1/capabilities`.
5. Generate via `POST /sdcpp/v1/img_gen`, poll `GET /sdcpp/v1/jobs/{id}`.
6. Decode `b64_json` and save to `<project>/assets/img/<id>.png`.
7. Write an `assets` row and emit `StudioEvent::AssetGenerated`.
8. On app quit, stop `sd-server`.

## Open questions / unresolved

1. Does `sd-server` support model hot-swapping? Source suggests the pipeline is
   loaded at startup; no reload endpoint was found.
2. Can we get per-step progress without parsing logs? Current API is poll-only.
3. Exact license terms for SD3.5 and Flux Dev — need legal review before any
   bundling or automated download.
4. Do we bundle prebuilt `sd-server` binaries or download them on first run?
   (Download is smaller; bundling is more reliable.)
