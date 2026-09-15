# /new-video

Create a new video project in Amaara Studio.

## Intent Interview

Run the intent interview to gather:
1. Subject? e.g. "black holes", "our Q3 launch", a URL…
2. Length & format? 30s faceless explainer / 60s promo / title card …
3. Tone & palette? calm + navy / bold + neon / minimal …
4. Narration? TTS voiceover / captions only / music bed …

Write `BRIEF.md` and `STORYBOARD.md`, then scaffold a HyperFrames project via `npx hyperframes init`.

---

# /render

Queue a video render in Amaara Studio's render queue.

## Usage

Call the studio tool `render_to_video(project_id, composition_id, quality, target)` where:
- quality: `draft` or `high`
- target: `local`, `docker`, `cloud`, `lambda`, `cloudrun`

Do NOT run raw `bash npx hyperframes render`. The studio's render sidecar handles HyperFrames/Remotion rendering via a Node 22 process with progress streaming to the UI render queue.

---

# /use-local

Switch generation source to Local in Amaara Studio.

## Usage

Call the studio tool `set_generation_source("local")` to route future `generate_image` calls to the local sd-server instead of Amaara Cloud. The status strip will show the sd-server sidecar status (idle→busy→idle).

---

# /use-cloud

Switch generation source to Cloud in Amaara Studio.

## Usage

Call the studio tool `set_generation_source("cloud")` to route future `generate_image` calls to Amaara Cloud `/v1/images/generations`. This is the default source on new projects (cloud-first).
