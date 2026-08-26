# Navya Cloud API Contracts — Research Report

**Scope:** Pin the exact API contracts the Navya Studio client must implement when talking to Navya Cloud as an OpenAI-compatible provider.  
**Sources inspected:**
- `C:/Users/hamis/Downloads/code/a-coder/provider-api/backend/main.py`
- `C:/Users/hamis/Downloads/code/a-coder/provider-api/backend/routers/{media,audio,embeddings,models,chat,proxy,byok}.py`
- `C:/Users/hamis/Downloads/code/a-coder/provider-api/backend/auth.py`
- `C:/Users/hamis/Downloads/code/a-coder/provider-api/backend/services/auth_context.py`
- `C:/Users/hamis/Downloads/code/a-coder/provider-api/backend/config.py`
- `C:/Users/hamis/Downloads/code/a-coder/provider-api/backend/.env.example`
- `C:/Users/hamis/Downloads/code/a-coder/provider-api/backend/schemas.py`
- `C:/Users/hamis/Downloads/code/a-coder/provider-api/backend/services/router.py`
- `C:/Users/hamis/Downloads/code/a-coder/provider-api/backend/services/provider_resolver.py`
- `C:/Users/hamis/Downloads/code/a-coder/provider-api/backend/services/rate_limiter.py`
- `C:/Users/hamis/Downloads/code/a-coder/provider-api/plan/BUILD-GAPS.md` §B
- Context7 OpenAI OpenAPI reference for comparison.

---

## 1. Base URL

| Item | Value / Rule |
|------|--------------|
| **Local dev default** | `http://localhost:8000` — the uvicorn server in `backend/main.py` binds to `host="0.0.0.0", port=8000`, and `.env.example` / `docker-compose.yml` both default `APP_BASE_URL` to `http://localhost:8000`. |
| **Production / deployed URL** | Not hard-coded. The deployed API lives at a public HTTPS origin (documented landing-page examples use `https://api.navya.ai/v1`). Studio should treat the API base URL as a user-configurable setting. |
| **OpenAI SDK base_url** | Set to `<origin>/v1`, e.g. `http://localhost:8000/v1` locally. All OpenAI-compatible routes are registered under `/v1/...` directly on the FastAPI app (`/v1/chat/completions`, `/v1/models`, `/v1/images/generations`, etc.). |
| **Relevant env vars** | `APP_BASE_URL` is used only for email magic-link/password-reset links, not for SDK routing. `CHUTES_BASE_URL` is the upstream default (`https://llm.chutes.ai/v1`), not the Navya client URL. There is **no dedicated `NAVYA_API_BASE_URL` or `API_BASE_URL` env var** for the public API. |

**Pin for BUILD-GAPS.md §B.7:** Default for dev = `http://localhost:8000`; client SDK base_url should be `<configured origin>/v1`; the value is user-entered in Studio settings.

---

## 2. Auth header format

The dependency `get_current_org_any` (`backend/services/auth_context.py`) resolves **both** user credentials and organization API keys from the same headers.

| Header | Semantics |
|--------|-----------|
| `Authorization: Bearer <token>` | `<token>` is first tried as a JWT access token (`/login` returns `Token` with `token_type: "bearer"`). If JWT validation fails, the same value is tried as a `User.api_key` or `OrganizationApiKey.api_key`. |
| `Authorization: <raw-key>` | Raw value (no `Bearer ` prefix) is also accepted and tried as an API key. |
| `X-API-Key: <key>` | Anthropic-style header; tried as an API key if `Authorization` does not resolve a user. |

For the Studio's OpenAI-compatible HTTP client the correct shape is:

```http
Authorization: Bearer <navya-api-key>
```

where `<navya-api-key>` is the per-user key stored in `users.api_key` (generated at registration by the auth router, not shown to the user as a secret in the code path inspected). Org-scoped keys from `organization_api_keys.api_key` are accepted the same way.

**Pin for BUILD-GAPS.md §B.8:** OpenAI-style `Authorization: Bearer <key>` is the expected header; the backend also tolerates raw `Authorization` and `X-API-Key`. The key is the user's Navya API key, not a provider upstream key.

---

## 3. `GET /public-models`, `GET /config`, `GET /health`

All three are **unauthenticated** read-only endpoints in `backend/main.py`.

### `GET /public-models`

Returns the anonymous landing-page catalog. Shape:

```json
{
  "models": [
    {
      "id": "model-id",
      "name": "Display Name",
      "context_window": 131072,
      "kind": "chat",
      "supports_vision": false,
      "strengths": ["coding", "reasoning", "long context"],
      "usage_level": 2
    }
  ],
  "total": 42
}
```

Details:
- Up to **12** models are returned (the rest are truncated for the landing-page widget).
- Models are sorted by descending quality score, then name.
- `kind` comes from `ProviderModel.kind` and can be `chat`, `image`, `video`, `tts`, `stt`, etc. (defaults to `"chat"`).
- `strengths` is the first three comma-separated, trimmed tokens of `ProviderModel.strengths`.
- Navya Engine (`provider_type == "navya_engine"`) models belonging to other orgs are excluded; the catalog is globally-public upstream models only.

### `GET /config`

```json
{ "chutes_api_key_configured": true }
```

Boolean reflects whether the server has a `CHUTES_API_KEY` env var. Useful for the Studio "Test connection" button.

### `GET /health`

```json
{ "status": "healthy", "service": "navya" }
```

**Pin for BUILD-GAPS.md §B.9:** Use `GET /public-models` to populate the model picker, `GET /config` + `GET /health` for connection checks.

---

## 4. `navya/auto` router behavior

| Item | Value / Rule |
|------|--------------|
| **Magic model id** | `"navya/auto"` (`AUTO_MODEL_ID` in `routers/proxy.py` and `routers/chat.py`). |
| **Server-side toggle** | `AUTO_ROUTER_ENABLED` env var, default `true`. If disabled, the proxy returns `400 "Auto-router is disabled"`. |
| **Router brain** | Configurable master LLM. Defaults: `AUTO_ROUTER_MODEL=Qwen/Qwen3-32B-TEE`, `AUTO_ROUTER_FALLBACK_MODEL=Qwen/Qwen3-32B-TEE`, `AUTO_ROUTER_MAX_TOKENS=1024`. |
| **What it does** | When `model` is `"navya/auto"`, the server calls `services/router.select_model_for_request(db, user_id \| organization_id, payload)` which:<br>1. Loads active models filtered by the user's plan `max_model_level` and by `supports_vision` if the request contains image content.<br>2. Calls the master LLM with a prompt that includes model profiles (strengths, context window, price, latency, quality score, provider health) and the last user message.<br>3. Expects the LLM to return **only** JSON `{"model": "<model_id>", "reasoning": "..."}`.<br>4. If parsing fails or the chosen model is unknown, falls back to `AUTO_ROUTER_FALLBACK_MODEL`.<br>5. Logs the decision to `router_decisions`. |
| **BYOK interaction** | **BYOK is ignored for `navya/auto`**. Both `proxy.py` and `chat.py` pass `ctx=None` to `resolve_providers_for_model` for the auto-router call, so the system key pool is always used. The docs explicitly warn: *"Auto always uses Navya keys — `navya/auto` never routes through a BYOK key."* |
| **Client behavior** | Studio should list `navya/auto` like any other model id and send it as `"model": "navya/auto"`. No extra client logic is required. |
| **Response metadata** | Non-streaming responses inject `navya_routed_model` and `navya_routing_reasoning` into the JSON body. Streaming responses inject `model` into every SSE data line and add response headers `X-Navya-Routed-Model` and `X-Navya-Routing-Reasoning`. |

**Pin for BUILD-GAPS.md §B.10:** Treat `navya/auto` as a regular model id; routing is fully server-side.

---

## 5. `POST /v1/images/generations`

Implemented in `backend/routers/media.py`.

### Request

OpenAI-compatible JSON body (passed through verbatim):

```json
{
  "model": "dall-e-3",
  "prompt": "...",
  "n": 1,
  "size": "1024x1024",
  "quality": "standard",
  "response_format": "url"
}
```

- `model` is **required**; missing it returns `422 "model is required"`.
- `n`, `size`, `quality`, `response_format`, and any other OpenAI fields are forwarded as-is to the upstream provider.
- The request path forwarded upstream is `POST <provider.base_url>/images/generations`.

### Response

Navya returns the **provider response verbatim**:

```json
{
  "created": 1700000000,
  "data": [
    { "url": "https://..." },
    { "b64_json": "..." }
  ]
}
```

- Whether the response contains `url` or `b64_json` depends entirely on the upstream provider and the `response_format` the client sent.
- Navya does **not** wrap the response, convert formats, or provide a polling status endpoint.

### Billing

Because image responses usually omit usage, Navya synthesizes:

```
synthetic_tokens = approximate_tokens_from_text(prompt) * max(1, n)
usage = {
  "prompt_tokens": synthetic_tokens,
  "completion_tokens": 0,
  "total_tokens": synthetic_tokens
}
```

and bills request-credits from that.

**Pin for BUILD-GAPS.md §B.11:** The contract is pure OpenAI pass-through. Studio must support both `url` and `b64_json` response shapes and should not assume one. There is no Navya-side job polling.

---

## 6. `POST /v1/videos/generations`

Implemented in `backend/routers/media.py`.

### Request

Proposed OpenAI-style JSON body (forwarded verbatim):

```json
{
  "model": "sora-2",
  "prompt": "...",
  "duration": 5,
  "size": "1280x720",
  "n": 1,
  "response_format": "url"
}
```

- `model` is required.
- `duration` / `size` / `response_format` are forwarded as-is.
- Upstream path: `POST <provider.base_url>/videos/generations`.

### Response

Navya returns the **provider response verbatim**. The code comments state the expected shape is:

```json
{
  "created": 1700000000,
  "data": [
    { "url": "..." }
  ]
}
```

but also note:

> "Some providers return a job-id and require polling; we forward the provider's response as-is. Clients can poll the provider's status endpoint if needed. Navya stays a transparent proxy here."

### Billing

Synthetic token estimate:

```
synthetic_tokens = approximate_tokens_from_text(prompt) * max(1, n)
if video and duration:
    synthetic_tokens += int(duration) * 50   # rough 50 tokens/sec
```

**Pin for BUILD-GAPS.md §B.12:** The endpoint is a transparent proxy. The Studio cannot assume a synchronous URL response; it must be prepared to handle either a direct URL or a provider-specific job handle, and to poll the provider's own status endpoint if necessary. Navya does not expose a unified job-status endpoint.

---

## 7. `/v1/audio/*` and `/v1/embeddings` shapes

### `GET /v1/audio/voices`

Returns the local Pocket TTS voice list (Navya-specific):

```json
{
  "voices": [
    "alba", "anna", "azelma", "bill_boerst", "charles", "cosette",
    "eponine", "eve", "fantine", "george", "jane", "jean", "javert",
    "marius", "mary", "michael", "paul", "peter_yearsley",
    "stuart_bell", "vera"
  ]
}
```

### `POST /v1/audio/speech`

Request schema (`AudioSpeechRequest`):

```json
{
  "model": "tts-1",
  "input": "Hello world",
  "voice": "alba",
  "response_format": "wav",
  "speed": 1.0
}
```

- Defaults: `model="tts-1"`, `voice="alba"`, `response_format="wav"`, `speed=1.0`.
- Navya first tries an upstream TTS provider (`ProviderModel.kind == "tts"`). If that succeeds, it returns the upstream audio bytes with the upstream `Content-Type`.
- If no upstream provider is configured or it fails, it falls back to local `pocket_tts` and returns `audio/wav`.
- **Response is raw audio bytes**, not JSON.

### `POST /v1/audio/transcriptions`

Multipart/form-data parameters (OpenAI/Whisper-compatible):

| Field | Type | Default |
|-------|------|---------|
| `file` | binary UploadFile | required |
| `model` | string | `"whisper-1"` |
| `language` | string | `null` |
| `prompt` | string | `null` |
| `response_format` | string | `"json"` |
| `temperature` | float | `0.0` |

Navya first tries an upstream STT provider (`ProviderModel.kind == "stt"`) via `POST /v1/audio/transcriptions`; if that succeeds it returns the provider response verbatim (content-type preserved). If no upstream is available or it fails, it falls back to local `faster-whisper` and supports `response_format` values:

- `"json"` → `{"text": "..."}`
- `"text"` → plain text
- `"srt"` → SRT subtitles
- `"vtt"` → WebVTT subtitles

### `POST /v1/embeddings`

Request schema (`EmbeddingRequest`):

```json
{
  "model": "text-embedding-3-small",
  "input": "text to embed",
  "dimensions": 1536,
  "user": "optional-user-id"
}
```

- `input` may be a string or an array of strings.
- Optional fields are omitted from the upstream payload (`req.dict(exclude_none=True)`).
- Navya resolves providers, supports BYOK, retries/falls back across providers, and returns the upstream response verbatim:

```json
{
  "object": "list",
  "data": [
    {
      "object": "embedding",
      "embedding": [0.0, ...],
      "index": 0
    }
  ],
  "model": "text-embedding-3-small",
  "usage": {
    "prompt_tokens": 4,
    "total_tokens": 4
  }
}
```

If the upstream omits `usage`, Navya approximates tokens from the input text.

**Pin for BUILD-GAPS.md §B.13:**
- `/v1/audio/speech` → JSON request, **binary audio response**.
- `/v1/audio/transcriptions` → multipart request, response format depends on `response_format` (`json`/`text`/`srt`/`vtt`).
- `/v1/embeddings` → standard OpenAI request/response, `input` may be string or list.

---

## 8. BYOK router endpoints and Studio exposure

### BYOK management endpoints (`backend/routers/byok.py`)

All require a **user JWT** (`Depends(get_current_user)`), not an org API key.

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/byok/providers` | List active upstream providers the user may attach a key to. |
| GET | `/byok/keys` | List the current user's stored BYOK keys (API key masked). |
| POST | `/byok/keys` | Store a new BYOK key. |
| PUT | `/byok/keys/{key_id}` | Update label / base_url / active flag / key. |
| DELETE | `/byok/keys/{key_id}` | Delete a key. |
| POST | `/byok/keys/{key_id}/test` | Test the key by calling `GET /models` on the provider. |

### BYOK create/update request shape

```json
{
  "provider_id": 1,
  "label": "My OpenAI key",
  "api_key": "sk-...",
  "base_url": "https://api.openai.com/v1"
}
```

`base_url` is optional; if omitted the provider's configured base URL is used.

### BYOK runtime behavior

When a user has an active `UserProviderKey` for a provider that serves the requested model, `services/provider_resolver.resolve_providers_for_model` overlays the user's own key/base_url onto the resolved provider and marks it `byok=True`. BYOK then applies to:

- `POST /v1/chat/completions` (`routers/proxy.py`)
- `POST /v1/messages` (`routers/chat.py`)
- `POST /v1/embeddings` (`routers/embeddings.py`)
- `POST /v1/audio/speech` (`routers/audio.py`)
- `POST /v1/audio/transcriptions` (`routers/audio.py`)

BYOK requests:
- Bypass quota/RPM/concurrency enforcement.
- Are **not** billed by Navya (`usage_units = 0`, no Stripe meter event).
- Are still logged with `source="byok"` for analytics.
- **Never** fall back to a system key.

**BYOK does NOT apply to:**
- `navya/auto` (router always uses system keys).
- `POST /v1/images/generations` and `POST /v1/videos/generations` (`media.py` uses only the system provider key; no BYOK overlay is performed).

### Should the Studio expose BYOK?

**Yes.** The public Navya docs already advertise BYOK as a first-class feature, the management router exists and is user-scoped, and BYOK is supported on the primary text/audio/embedding paths. For v1 the Studio should:

1. Add a **Settings → BYOK Keys** UI that mirrors the five endpoints above.
2. Allow the user to pick a provider, label, paste the upstream key, and optionally override `base_url`.
3. Re-use those stored BYOK keys automatically when calling chat/embeddings/audio endpoints.
4. Not expose BYOK UI for image/video generation because the backend does not support it.

**Pin for BUILD-GAPS.md §B.14:** Studio **should** expose BYOK key management for chat, embeddings, and audio. Do not expose it for `navya/auto`, image generation, or video generation.

---

## 9. Rate-limit / usage headers

| Header | When present | Value |
|--------|--------------|-------|
| `X-Navya-Request-Id` | **Every response** | A per-request trace id generated in the `_EdgeMiddleware` (`backend/main.py`). |
| `X-Navya-Routed-Model` | Chat/audio/embeddings when auto-router chose a model | The concrete model id selected by `navya/auto`. |
| `X-Navya-Routing-Reasoning` | Chat/audio/embeddings when auto-router chose a model | The router's reasoning string (may be long). |
| `Retry-After` | On **RPM-pace** 429 responses only | Integer seconds until the next request slot is available (`services/rate_limiter.py` → `RPMPaceLimiter`). |

**Notably absent:**

- No `X-RateLimit-Limit`, `X-RateLimit-Remaining`, or `X-RateLimit-Reset` headers.
- No `X-Navya-Credits-Remaining` or `X-Navya-Usage` headers.
- The abuse backstop limiter (`apply_rate_limit` in `services/rate_limiter.py`) and the concurrency cap (`services/usage_enforcer.check_and_acquire_concurrency`) return **429 without `Retry-After`**.
- Budget / quota failures return **402** with a plain JSON/text detail, no usage headers.

**Pin:** Studio should surface `X-Navya-Request-Id` in error reporting, read `X-Navya-Routed-Model` when using `navya/auto`, and respect `Retry-After` only on the plan-aware RPM-pace 429s.

---

## 10. Summary of pinned answers for `plan/BUILD-GAPS.md` §B

| # | Question | Pinned answer |
|---|----------|---------------|
| 7 | Base URL | Dev: `http://localhost:8000`; client SDK base_url = `<origin>/v1`; value is user-configured in Studio settings. |
| 8 | Auth header | `Authorization: Bearer <navya-user-or-org-api-key>`; also accepts raw `Authorization` and `X-API-Key`. |
| 9 | Model discovery | `GET /public-models` returns `{models:[{id,name,context_window,kind,supports_vision,strengths,usage_level}],total}` (max 12). `GET /config` and `GET /health` for connection tests. |
| 10 | `navya/auto` | Pass through as a model id; server-side master LLM picks the concrete model, returns `X-Navya-Routed-Model` / `X-Navya-Routing-Reasoning`. |
| 11 | Image generation | OpenAI pass-through `POST /v1/images/generations`; response is provider verbatim `{data:[{url}\|{b64_json}]}`; Navya does not normalize. |
| 12 | Video generation | Transparent proxy `POST /v1/videos/generations`; may return synchronous URL or provider-specific job handle; Navya has no status endpoint. |
| 13 | Audio + embeddings | `/v1/audio/speech` (JSON in, audio bytes out), `/v1/audio/transcriptions` (multipart, supports json/text/srt/vtt), `/v1/embeddings` (OpenAI shape). |
| 14 | BYOK | Expose BYOK management in Studio for chat, embeddings, audio. Not for `navya/auto`, images, or videos. |
| 15 | API key | `[DATA]` — a real Navya API key + test account must be provided by the user and stored in the OS keyring; no secret values are present in the inspected codebase. |

---

*Report generated from codebase inspection and Context7 OpenAPI reference. No code was modified.*
