# Local LLM Serving with llama.cpp — Navya Studio Research

> **Build-gap item:** `plan/BUILD-GAPS.md` §D.25 — pin the `llama-server` contract, tool-call support, and a recommended tool-capable model for the Phase 9 local-LLM path.

This report documents the exact binary, HTTP contract, JSON request/response shapes, runtime tool-call detection, build flags, environment/config support, and VRAM guidance needed to integrate a local llama.cpp server into Navya Studio as an optional `Source` next to Navya Cloud.

---

## 1. The `llama-server` binary

| Item | Exact answer |
|------|--------------|
| Binary name | `llama-server` (Linux/macOS) / `llama-server.exe` (Windows) |
| Typical build output | `build/bin/llama-server` (single-config CMake) or `build/bin/Release/llama-server.exe` (Windows multi-config) |
| Docker image (official) | `ghcr.io/ggml-org/llama.cpp:server` (CPU) and `ghcr.io/ggml-org/llama.cpp:server-cuda` (CUDA) |
| Source reference | `tools/server/` in `ggml-org/llama.cpp` |

Navya Studio should treat `llama-server` as a user-provided or installer-shipped sidecar (similar to `sd-server`). The studio only needs to spawn it with a model path and then talk to its HTTP API on a chosen port.

### Minimum working invocation

```bash
llama-server \
  -m /path/to/model.gguf \
  -c 4096 \
  --port 8080 \
  --host 127.0.0.1 \
  -ngl 99 \
  --jinja
```

For a user-managed server, the studio may simply point at `http://127.0.0.1:<port>/v1` and skip spawning.

---

## 2. OpenAI-compatible HTTP endpoints

The server registers the following routes (from `tools/server/server.cpp`):

```text
GET  /health
GET  /v1/health
GET  /metrics
GET  /props
POST /props                    (only if --props enabled)
GET  /models
GET  /v1/models
POST /v1/completions           (legacy + OpenAI)
POST /v1/chat/completions
POST /v1/chat/completions/control
POST /v1/responses
POST /v1/audio/transcriptions
POST /v1/messages              (Anthropic-compatible)
POST /v1/embeddings
POST /v1/rerank
POST /tokenize
POST /detokenize
POST /apply-template
GET  /slots
POST /slots/:id_slot
GET  /lora-adapters
POST /lora-adapters
```

For Navya Studio the important subset is:

| Endpoint | Purpose | OpenAI-shaped? |
|----------|---------|----------------|
| `GET /health` | readiness / model-loading status | yes-ish (see below) |
| `GET /v1/models` | discover the loaded model | yes |
| `POST /v1/chat/completions` | chat + tool calls | yes |
| `GET /props` | runtime server/model metadata, **used for tool-call detection** | custom |
| `GET /metrics` | Prometheus metrics (requires `--metrics`) | Prometheus |

### `GET /health`

Returns while the model is loading:

```json
{"error": {"code": 503, "message": "Loading model", "type": "unavailable_error"}}
```

Returns when ready:

```json
{"status": "ok"}
```

### `GET /v1/models`

Response shape:

```json
{
  "object": "list",
  "data": [
    {
      "id": "../models/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf",
      "object": "model",
      "created": 1735142223,
      "owned_by": "llamacpp",
      "meta": {
        "vocab_type": 2,
        "n_vocab": 128256,
        "n_ctx_train": 131072,
        "n_embd": 4096,
        "n_params": 8030261312,
        "size": 4912898304
      }
    }
  ]
}
```

The `id` defaults to the model file path unless an `--alias` is set. The studio should display `id` as the model name and use `meta.size` to warn about VRAM.

### `GET /props`

Response shape (truncated to the fields Navya Studio cares about):

```json
{
  "default_generation_settings": { ... },
  "total_slots": 1,
  "model_path": "../models/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf",
  "chat_template": "...",
  "chat_template_caps": {},
  "modalities": { "vision": false },
  "media_marker": "<__media_...__>",
  "build_info": "b(build number)-(commit hash)",
  "is_sleeping": false
}
```

The `chat_template_caps` object is the key to detecting tool support (see §4).

---

## 3. Request/response JSON shapes

### `POST /v1/chat/completions` — non-streaming

**Request fields** (OpenAI-compatible):

```json
{
  "model": "gpt-3.5-turbo",
  "messages": [
    {"role": "system", "content": "You are an AI assistant."},
    {"role": "user", "content": "Write a limerick about python exceptions"}
  ],
  "max_tokens": 512,
  "temperature": 0.8,
  "top_p": 0.95,
  "stream": false,
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_current_weather",
        "description": "Get the current weather in a given location",
        "parameters": {
          "type": "object",
          "properties": {
            "location": {
              "type": "string",
              "description": "The city and country/state, e.g. San Francisco, CA"
            }
          },
          "required": ["location"]
        }
      }
    }
  ],
  "tool_choice": "auto",
  "parallel_tool_calls": false,
  "parse_tool_calls": true
}
```

Observed server-specific fields in the request body:

- `parse_tool_calls` — whether to parse generated tool calls.
- `parallel_tool_calls` — whether to allow parallel tool calls.
- `chat_template_kwargs` — extra params passed to the Jinja template parser.
- `reasoning_effort`, `reasoning_format`, `reasoning_control` — reasoning/thinking controls.

**Response shape (text completion):**

```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion",
  "created": 1735142223,
  "model": "gpt-3.5-turbo",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "..."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 44,
    "completion_tokens": 16,
    "total_tokens": 60
  }
}
```

**Response shape when a tool is called (from `docs/function-calling.md`):**

```json
{
  "id": "chatcmpl-Htbgh9feMmGM0LEH2hmQvwsCxq3c6Ni8",
  "object": "chat.completion",
  "created": 1727287211,
  "model": "gpt-3.5-turbo",
  "choices": [
    {
      "index": 0,
      "finish_reason": "tool",
      "message": {
        "role": "assistant",
        "content": null,
        "tool_calls": [
          {
            "name": "python",
            "arguments": "{\"code\": \" \\nprint(\\\"Hello, World!\\\")\"}"
          }
        ]
      }
    }
  ],
  "usage": {
    "prompt_tokens": 44,
    "completion_tokens": 16,
    "total_tokens": 60
  }
}
```

Note: llama.cpp’s tool-call object is `{name, arguments}` rather than the strict OpenAI `{id, type, function: {name, arguments}}`. The studio parser must accept either shape and, if `id` is missing, synthesize a `tool_call_id` so the subsequent `role: "tool"` message can reference it.

### Streaming (`stream: true`)

The server returns Server-Sent Events. Each non-final chunk follows the OpenAI `chat.completion.chunk` convention:

```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion.chunk",
  "created": 1735142223,
  "model": "gpt-3.5-turbo",
  "system_fingerprint": "b...-...",
  "choices": [
    {
      "index": 0,
      "delta": {
        "role": "assistant",
        "content": "hello"
      },
      "finish_reason": null
    }
  ]
}
```

Tool calls stream incrementally through the same `delta` object. The internal diff struct updates `tool_call_delta.name` and `tool_call_delta.arguments`, so the SSE chunk is expected to contain `delta.tool_calls` with partial function data.

If `stream_options.include_usage: true` is sent, the final SSE chunk has an empty `choices` array and a `usage` object:

```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion.chunk",
  "created": 1735142223,
  "model": "gpt-3.5-turbo",
  "system_fingerprint": "b...-...",
  "choices": [],
  "usage": {
    "prompt_tokens": 44,
    "completion_tokens": 16,
    "total_tokens": 60
  }
}
```

### `role: "tool"` result messages

After a tool runs, append a message with `role: "tool"`, the matching `tool_call_id`, and the stringified JSON result:

```json
{
  "role": "tool",
  "tool_call_id": "call_abc123",
  "content": "{\"temperature\": 72, \"unit\": \"fahrenheit\"}"
}
```

The `common_chat_msg` struct supports mixed content (an assistant message can carry both `content` and `tool_calls`), so the studio should preserve all `tool_calls` arrays from the server rather than collapsing them into text.

---

## 4. Tool-calling support

### Requirement

The server supports OpenAI-style function calling **only when started with `--jinja`**. Without `--jinja`, the tool/chat template path is not active.

From `tools/server/README.md`:

> The server supports OpenAI-style function calling when using the `--jinja` flag. Depending on the model, users may need to provide a specific `--chat-template-file` or use the `--chat-template chatml` option to ensure compatibility with tool-use templates.

### Native vs. generic handlers

Native tool-call templates are recognized for:

- Llama 3.1 / 3.3
- Functionary v3.1 / v3.2
- Hermes 2 / 3
- Qwen 2.5
- Mistral Nemo
- Firefunction v2
- Command R7B
- DeepSeek R1

If a template is not natively recognized, llama.cpp falls back to a **generic** tool-call parser. Generic mode works with all models but consumes more tokens and is less reliable. For best results the studio should recommend a natively-supported model and, when possible, a matching `--chat-template-file`.

### How to detect tool-calling support at runtime

Call `GET /props` and inspect `chat_template_caps`. The capabilities struct (defined in `common/jinja/caps.h`) is exposed as a map:

```cpp
struct caps {
    bool supports_tools = true;
    bool supports_tool_calls = true;
    bool supports_system_role = true;
    bool supports_parallel_tool_calls = true;
    bool supports_preserve_reasoning = false;
    bool supports_reasoning_effort = false;
    bool supports_string_content = true;
    bool supports_typed_content = false;
    bool supports_object_arguments = false;
    std::map<std::string, bool> to_map() const;
};
```

A model whose Jinja template is tool-aware will return:

```json
{
  "chat_template_caps": {
    "supports_tools": true,
    "supports_tool_calls": true,
    "supports_system_role": true,
    "supports_parallel_tool_calls": true
  }
}
```

If the map is empty or `supports_tools`/`supports_tool_calls` are `false`, the server will either refuse tool calls or use the generic parser. Navya Studio should:

1. Always start the server with `--jinja`.
2. Poll `GET /props` after `/health` is ready.
3. Warn the user if `chat_template_caps.supports_tools !== true`.
4. Block or downgrade the local agent path when the server was started without `--jinja`.

### Forcing a tool-aware template

If the GGUF does not embed the correct template, override it:

```bash
# Use one of the built-in aliases
llama-server --jinja --chat-template chatml -m model.gguf

# Or provide a Jinja file (e.g. from scripts/get_chat_template.py)
llama-server --jinja \
  --chat-template-file models/templates/NousResearch-Hermes-3-Llama-3.1-8B-tool_use.jinja \
  -m model.gguf
```

The repo provides a helper to download templates:

```bash
./scripts/get_chat_template.py NousResearch/Hermes-3-Llama-3.1-8B tool_use \
  > models/templates/NousResearch-Hermes-3-Llama-3.1-8B-tool_use.jinja
```

---

## 5. Command-line arguments to start the server

The following flags are the minimum set Navya Studio needs to expose or hard-code. Defaults are taken from `common/arg.cpp` and `tools/server/README.md`.

| Flag | Short | Env var | Default | Meaning |
|------|-------|---------|---------|---------|
| `--model` | `-m` | `LLAMA_ARG_MODEL` | — | Path to the `.gguf` model file |
| `--ctx-size` | `-c` | `LLAMA_ARG_CTX_SIZE` | `0` (load from model) | Context window size |
| `--n-gpu-layers` | `-ngl` | `LLAMA_ARG_N_GPU_LAYERS` (assumed) | `0` | Layers offloaded to GPU; accepts `auto` or `all` |
| `--main-gpu` | `-mg` | `LLAMA_ARG_MAIN_GPU` | `0` | GPU used when `split-mode=none` |
| `--split-mode` | — | `LLAMA_ARG_SPLIT_MODE` | `layer` | `none`, `layer`, `row`, `tensor` |
| `--tensor-split` | — | `LLAMA_ARG_TENSOR_SPLIT` | — | Fractional split across GPUs |
| `--device` | — | `LLAMA_ARG_DEVICE` | — | Restrict to specific device name(s) |
| `--port` | — | `LLAMA_ARG_PORT` | `8080` | TCP port |
| `--host` | — | `LLAMA_ARG_HOST` | `127.0.0.1` | Bind address (or `.sock` UNIX socket) |
| `--parallel` | `-np` | `LLAMA_ARG_N_PARALLEL` | `1` | Server slots (concurrent requests) |
| `--threads` | `-t` | `LLAMA_ARG_THREADS` | `-1` → `hardware_concurrency` | CPU threads for generation |
| `--threads-http` | — | `LLAMA_ARG_THREADS_HTTP` | auto | HTTP worker threads |
| `--flash-attn` | `-fa` | `LLAMA_ARG_FLASH_ATTN` | off | Flash Attention (saves KV memory) |
| `--jinja` | — | `LLAMA_ARG_JINJA` (assumed) | off | Enable Jinja chat templates (required for tools) |
| `--chat-template` | — | `LLAMA_ARG_CHAT_TEMPLATE` | — | Built-in template alias, e.g. `chatml` |
| `--chat-template-file` | — | `LLAMA_ARG_CHAT_TEMPLATE_FILE` | — | Path to a custom `.jinja` file |
| `--no-webui` / `--no-ui` | — | `LLAMA_ARG_UI` | web UI enabled | Disable the bundled web UI |
| `--metrics` | — | `LLAMA_ARG_ENDPOINT_METRICS` | off | Enable `GET /metrics` |
| `--props` | — | `LLAMA_ARG_ENDPOINT_PROPS` | off | Allow `POST /props` |
| `--api-key` | — | `LLAMA_API_KEY` | none | Optional bearer-token auth |
| `--api-key-file` | — | `LLAMA_ARG_API_KEY_FILE` | none | File with keys, one per line |
| `--cache-prompt` | — | `LLAMA_ARG_CACHE_PROMPT` | on | KV-cache prompt reuse |
| `--no-warmup` | — | `LLAMA_ARG_NO_WARMUP` | off | Skip warmup (useful in Docker) |
| `--timeout` | `-to` | `LLAMA_ARG_TIMEOUT` | `600` | Read/write timeout in seconds |

Recommended studio-side invocation for a tool-calling model:

```bash
llama-server \
  -m "${MODEL}" \
  -c 8192 \
  --port "${PORT}" \
  --host 127.0.0.1 \
  -ngl all \
  -fa on \
  --jinja \
  --no-webui \
  --metrics \
  --props \
  --np 1
```

Notes:

- `-ngl all` (or a large number like `99`) offloads every layer that fits in VRAM. If the build has no GPU backend, the flag is ignored with a warning.
- `-fa on` reduces KV-cache memory and increases throughput; highly recommended for long contexts.
- `--jinja` is mandatory for tool calls.
- `--np 1` matches a single Studio conversation. Increase only if the user explicitly wants parallel local jobs.

---

## 6. Build flags for CUDA/Vulkan/CPU and runtime GPU detection

llama.cpp supports multiple GPU backends. The backend registry is fixed at compile time; runtime selection is limited to which devices of a compiled backend are visible.

### Common CMake build commands

| Backend | CMake configure command |
|---------|--------------------------|
| CPU (default) | `cmake -B build && cmake --build build --config Release` |
| CUDA | `cmake -B build -DGGML_CUDA=ON && cmake --build build --config Release` |
| Vulkan | `cmake -B build -DGGML_VULKAN=1 -DGGML_METAL=OFF && cmake --build build --config Release` |
| Metal (macOS) | `cmake -B build -DGGML_METAL=ON && cmake --build build --config Release` |
| HIP/ROCm | `HIPCXX="$(hipconfig -l)/clang" HIP_PATH="$(hipconfig -R)" cmake -B build -DGGML_HIP=ON -DGPU_TARGETS=gfx1030 -DCMAKE_BUILD_TYPE=Release && cmake --build build --config Release` |
| SYCL (Intel) | `cmake -B build -DGGML_SYCL=ON -DCMAKE_C_COMPILER=icx -DCMAKE_CXX_COMPILER=icpx -DGGML_SYCL_F16=ON && cmake --build build --config Release` |
| OpenCL | `cmake -B build -DGGML_OPENCL=ON ...` |

Multiple backends can be compiled into the same binary, e.g.:

```bash
cmake -B build -DGGML_CUDA=ON -DGGML_VULKAN=ON
```

### Runtime GPU detection

1. **Backend registration** happens at startup via `#ifdef GGML_USE_*` blocks in `ggml/src/ggml-backend-reg.cpp`. If CUDA was not compiled in, there is no CUDA path regardless of hardware.
2. **Device availability** is discovered by each registered backend. The server logs which device it uses and how many layers were offloaded:
   ```text
   llama_model_load_internal: [cublas] offloading 60 layers to GPU
   llama_model_load_internal: [cublas] offloading output layer to GPU
   llama_model_load_internal: [cublas] total VRAM used: 17223 MB
   ```
3. **No-GPU fallback** is automatic: if `-ngl` is larger than the number of layers or no GPU is found, remaining layers run on CPU. If the binary has no GPU backend at all, `-ngl` prints a warning and CPU is used.
4. **Force-disable a backend** with environment variables such as `GGML_DISABLE_VULKAN=1`.

### Installer recommendation for Navya Studio

Because GPU support is mostly compile-time, the installer should ship one flavor per target and pick the smallest compatible one:

1. Detect NVIDIA driver / `nvidia-smi` → ship `llama-server-cuda`.
2. Else detect Vulkan ICD (`vulkaninfo` / registry) → ship `llama-server-vulkan`.
3. Else ship `llama-server-cpu`.

At runtime the studio can double-check by parsing the first few lines of `llama-server` stderr for `[cublas]`, `Vulkan`, `Metal`, etc., and by confirming `/health` returns `{"status":"ok"}`.

---

## 7. Environment variables and config files

### `LLAMA_ARG_*` environment variables

Almost every CLI option in `common/arg.cpp` is bound to an environment variable. Known bindings used by the server:

- `LLAMA_ARG_MODEL`
- `LLAMA_ARG_MODEL_URL` (download from URL)
- `LLAMA_ARG_CTX_SIZE`
- `LLAMA_ARG_N_PARALLEL`
- `LLAMA_ARG_THREADS`
- `LLAMA_ARG_THREADS_HTTP`
- `LLAMA_ARG_PORT`
- `LLAMA_ARG_HOST`
- `LLAMA_ARG_MAIN_GPU`
- `LLAMA_ARG_N_GPU_LAYERS` (expected binding, used like `-ngl`)
- `LLAMA_ARG_FLASH_ATTN`
- `LLAMA_ARG_CHAT_TEMPLATE`
- `LLAMA_ARG_CHAT_TEMPLATE_FILE`
- `LLAMA_ARG_CHAT_TEMPLATE_KWARGS`
- `LLAMA_ARG_UI`
- `LLAMA_ARG_ENDPOINT_METRICS`
- `LLAMA_ARG_ENDPOINT_PROPS`
- `LLAMA_ARG_ENDPOINT_SLOTS`
- `LLAMA_ARG_CACHE_PROMPT`
- `LLAMA_ARG_CACHE_REUSE`
- `LLAMA_ARG_TIMEOUT`
- `LLAMA_ARG_SSE_PING_INTERVAL`
- `LLAMA_ARG_CORS_ORIGINS`, `LLAMA_ARG_CORS_METHODS`, `LLAMA_ARG_CORS_HEADERS`, `LLAMA_ARG_CORS_CREDENTIALS`
- `LLAMA_ARG_API_PREFIX`
- `LLAMA_ARG_STATIC_PATH`
- `LLAMA_ARG_API_KEY_FILE`
- `LLAMA_API_KEY` (note: **not** `LLAMA_ARG_API_KEY`) for `--api-key`
- `LLAMA_ARG_TOOLS`, `LLAMA_ARG_TOOLS_RUNTIME`, `LLAMA_ARG_AGENT`
- `LLAMA_ARG_MCP_SERVERS_CONFIG`, `LLAMA_ARG_MCP_SERVERS_JSON`
- `LLAMA_ARG_LOG_COLORS`, `LLAMA_ARG_LOG_VERBOSITY`, `LLAMA_ARG_LOG_PREFIX`, `LLAMA_ARG_LOG_TIMESTAMPS`

**Precedence:** system/user INI preset → environment variable → command-line argument → model-specific preset. If both an env var and a CLI flag are set for the same option, the CLI flag wins and a warning is printed.

### Config / preset files

llama.cpp supports INI-style model presets loaded with `--models-preset <file>`:

```ini
[*]
ctx-size = 0
mmap     = 1
parallel = 4

[Qwen2.5-7B-Instruct-GGUF]
hf        = bartowski/Qwen2.5-7B-Instruct-GGUF:Q4_K_M
ctx-size  = 8192
batch-size = 2048
temp      = 0.8
```

Example server start:

```bash
llama-server --models-preset ./my-models.ini
```

The studio can generate a per-project preset file with the selected model/context and pass it to the spawned server. The preset is overridden by any explicit CLI flags the user adds.

### Docker Compose example

```yaml
services:
  llamacpp-server:
    image: ghcr.io/ggml-org/llama.cpp:server
    ports:
      - "8080:8080"
    volumes:
      - ./models:/models
    environment:
      LLAMA_ARG_MODEL: /models/my_model.gguf
      LLAMA_ARG_CTX_SIZE: 4096
      LLAMA_ARG_N_PARALLEL: 2
      LLAMA_ARG_ENDPOINT_METRICS: 1
      LLAMA_ARG_PORT: 8080
```

---

## 8. Performance / VRAM guidance for tool-capable models

### Quantization and model size

For local tool use, the recommended format is **GGUF Q4_K_M**. It is the best balance of quality, file size, and VRAM. The `/v1/models` response already exposes `meta.size` (bytes on disk), which is a lower bound for VRAM.

Approximate VRAM when offloading **all** layers (`-ngl all`) at Q4_K_M:

| Model family | Params | Q4_K_M file ≈ | All-layers VRAM floor (with overhead) | Notes |
|--------------|--------|---------------|----------------------------------------|-------|
| Qwen2.5 Instruct | 7B | ~4.5 GB | ~5.5–6 GB | Excellent native tool support |
| Hermes 3 / Llama 3.1 | 8B | ~5 GB | ~6–7 GB | Strong tool templates |
| Mistral Nemo | 12B | ~7.5 GB | ~9–10 GB | Higher quality, needs more VRAM |
| Llama 3.1 / 3.3 | 70B | ~40 GB | ~45–50 GB | High-end GPU only |

These are estimates; actual use is higher because of:

- KV-cache memory (scales with context size),
- Flash Attention overhead savings,
- CUDA/Vulkan runtime allocations,
- Any speculative decoding or parallel slots.

### Context-size VRAM rule of thumb

The KV cache size per token is roughly:

```text
per-token-bytes ≈ (n_embd_k_gqa + n_embd_v_gqa) * n_layers * kv_type_size
```

With the default `f16` KV type, that is 2 bytes per element. For a 7–8B model, this is roughly **0.1–0.2 MB per token**; a 32k context can add **3–6 GB** on top of the model weights.

Ways to reduce KV memory:

- Use `-fa on` / `--flash-attn`.
- Lower `--ctx-size` to the smallest value the project needs (e.g. 4096 or 8192).
- Reduce KV precision with `--cache-type-k q8_0` / `--cache-type-v q8_0` (trades quality for memory).
- Keep `--parallel 1` unless the user explicitly wants concurrent requests.

### Recommended defaults for Navya Studio

For a typical 8B tool-calling model on a consumer GPU:

```bash
llama-server \
  -m model-Q4_K_M.gguf \
  -c 8192 \
  -ngl all \
  -fa on \
  --jinja \
  --np 1 \
  --no-webui \
  --port 8080
```

Expected minimum VRAM: **~6 GB**. For CPU-only machines, the same command works but generation is slow; context should be reduced to `-c 4096` or lower.

### Tool-capable model recommendations

Models that have **native** tool templates in llama.cpp (source: `docs/function-calling.md`):

1. **Qwen2.5-7B-Instruct-GGUF:Q4_K_M** — best small-model choice for tool use.
2. **Hermes-3-Llama-3.1-8B-GGUF:Q4_K_M** — strong alternative; may need `--chat-template-file` override.
3. **Llama-3.1-8B-Instruct-GGUF:Q4_K_M** or **Llama-3.2-3B-Instruct-GGUF** — good general quality; native tool support in 3.1/3.3.
4. **Mistral-Nemo-Instruct-2407-GGUF:Q6_K_L** — larger, better reasoning, needs ~10 GB VRAM.
5. **Functionary-small-v3.2-GGUF:Q4_K_M** — purpose-built for function calling.
6. **DeepSeek-R1-Distill-Qwen/Llama** — if reasoning + tools are desired.

For the default local model in Navya Studio, recommend **Qwen2.5-7B-Instruct Q4_K_M** as the minimum viable tool-calling model, with **Hermes-3-Llama-3.1-8B Q4_K_M** as the fallback if the user prefers a Llama-family model.

---

## 9. Studio integration checklist

1. **Binary discovery** — look for `llama-server` (or the per-target sidecar name) on PATH or in the bundled sidecar dir.
2. **Flavor selection at install time** — ship CUDA/Vulkan/CPU binaries using the same detection logic planned for `sd-server`.
3. **Spawn command** — pass `-m`, `-c`, `--port`, `--host`, `-ngl all`, `-fa on`, `--jinja`, `--no-webui`, `--metrics`, `--props`.
4. **Readiness** — poll `GET /health` until `{"status":"ok"}`.
5. **Tool-support validation** — call `GET /props` and check `chat_template_caps.supports_tools === true`. Warn if false or if the server was not started with `--jinja`.
6. **Model info** — use `GET /v1/models` to show the model id and `meta.size`. Warn if VRAM looks insufficient.
7. **Chat adapter** — treat `POST /v1/chat/completions` as an OpenAI-compatible endpoint, but tolerate llama.cpp’s `{name, arguments}` tool-call shape and missing `id`.
8. **Streaming** — consume SSE chunks; request `stream_options.include_usage: true` if token accounting is needed.
9. **Secrets / auth** — if the user sets an API key, pass it as `Authorization: Bearer <key>` matching `--api-key` / `LLAMA_API_KEY`.
10. **Project preset** — generate a per-project `.ini` preset via `--models-preset` so context size and model are reproducible.

---

## 10. Sources

- `ggml-org/llama.cpp` Context7 library (`/ggml-org/llama.cpp`), primarily:
  - `tools/server/README.md`
  - `tools/server/server.cpp` (route table)
  - `docs/function-calling.md`
  - `docs/build.md`
  - `docs/preset.md`
  - `docs/multi-gpu.md`
  - `docs/development/token_generation_performance_tips.md`
  - `common/arg.cpp`
  - `common/jinja/caps.h`
  - `common/chat.h`
  - `ggml/src/ggml-backend-reg.cpp`
