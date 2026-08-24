# Navya Studio Skill

Use this skill when working with the Navya Studio content-creation studio. The studio is a Tauri desktop app powered by AI that orchestrates script/storyboard writing, image generation, and video rendering via HyperFrames/Remotion.

## Core Concepts

### Cloud vs Local Source
Navya Studio supports two generation sources:
- **Cloud (default)**: Navya Cloud AI models (remote, OpenAI-compatible) via `/v1/chat/completions`, `/v1/images/generations`, etc. Billed by Navya.
- **Local**: Local LLM + stable-diffusion.cpp `sd-server`. Free, user's GPU/CPU.

The studio's `Source` toggle in the UI drives where `generate_image` routes. Default is Cloud (cloud-first).

## Studio Tools

When working in Navya Studio, use these tools via the a-coder-cli extension:

- `generate_image(project_id, composition_id?, prompt, model?, size?)`: Generate an image via Navya Cloud or local sd-server. Returns `GeneratedImage` with asset record created in the store.
- `render_to_video(project_id, composition_id, quality, target)`: Queue a video render via the HyperFrames render sidecar. Quality: `draft` or `high`. Target: `local`, `docker`, `cloud`, `lambda`, `cloudrun`. Returns `RenderJob` id.
- `list_local_models()`: List available cloud and local models.
- `set_generation_source(source)`: Set the generation source (`cloud` or `local`).
- `get_project_state()`: Get current project state from the store (project_id, name, harness, model, source, compositions_count, assets_count).
- `snapshot(timecode_ms)`: Snapshot a frame at timecode t (ms) from the current composition. Returns `Snapshot` with asset record.

## Video Rendering Note

**Important**: Renders go through `render_to_video` — do NOT run raw `bash npx hyperframes render`. The studio's render sidecar handles HyperFrames/Remotion rendering via a Node 22 process that runs `npx hyperframes …` or `@remotion/renderer`, with progress streaming back to the UI as render-queue events.

## Authoring Videos

When authoring videos/compositions, point the agent at the **HyperFrames skills**:
- `/hyperframes` — mandatory entry point for any video/animation/motion graphic task
- `hyperframes-core` — composition contract, `data-*` timing attributes, tracks, variables
- `hyperframes-animation` — atomic motion rules, multi-phase scene blueprints, runtime adapters (GSAP, Lottie, Three.js, Anime.js, CSS keyframes, WAAPI, TypeGPU)
- `faceless-explainer`, `product-launch-video`, `talking-head-recut` — specialized video workflows

Use these skills for video authoring; use Navya Studio tools (`generate_image`, `render_to_video`) for generation and rendering coordination.
