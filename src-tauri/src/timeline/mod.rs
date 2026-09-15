//! Timeline composition parser (Phase 10).
//!
//! Parses a HyperFrames composition's HTML into a normalized timeline: a list
//! of clips plus tracks grouped by `data-track-index`. This is the real
//! implementation behind `commands::timeline::get_timeline`; the earlier mock
//! returned hand-written tracks, so the Timeline tab had nothing to show for a
//! real project.
//!
//! **Contract (see `docs/research/hyperframes-render-contract.md`):** a
//! composition is a set of HTML files. Clips are elements with
//! `class="clip"` or a `data-composition-src` attribute. Required timing
//! attributes:
//!
//! | attribute              | meaning                              |
//! |------------------------|--------------------------------------|
//! | `data-composition-id`  | unique id for this clip              |
//! | `data-composition-src` | path to the block/component HTML     |
//! | `data-start`           | start time in **seconds**            |
//! | `data-duration`        | duration in **seconds**              |
//! | `data-track-index`     | layer / z-order                      |
//! | `data-width`           | width in pixels                      |
//! | `data-height`          | height in pixels                     |
//! | `data-composition-variables` | JSON object of composition vars |
//!
//! Composition files may reference sub-compositions via `data-composition-src`;
//! `load_timeline` resolves those recursively (bounded depth) so nested
//! blocks contribute their own clips.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// A single clip/block parsed from a HyperFrames composition's HTML.
///
/// `PartialEq` only (not `Eq`): the timing fields are `f64`, whose equality is
/// not total, but the derived structural equality is sufficient for tests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Clip {
    /// `data-composition-id`.
    pub id: String,
    /// `data-composition-src` (relative path to the block HTML), if present.
    pub src: Option<String>,
    /// Start time in seconds (`data-start`).
    pub start_s: f64,
    /// Duration in seconds (`data-duration`).
    pub duration_s: f64,
    /// Layer / z-order (`data-track-index`).
    pub track_index: i32,
    /// `data-width`, if present.
    pub width: Option<u32>,
    /// `data-height`, if present.
    pub height: Option<u32>,
    /// Raw `data-composition-variables` JSON string, if present.
    pub variables: Option<String>,
}

/// A rendered track: clips grouped by `data-track-index`, ordered by start time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineTrack {
    /// The `data-track-index` this track represents.
    pub index: i32,
    /// Derived short name (from the first clip's id or src basename).
    pub name: String,
    /// Human label (same as name here; reserved for future nicety).
    pub label: String,
    /// Start of the track in milliseconds.
    pub start_ms: i64,
    /// Duration of the track in milliseconds.
    pub duration_ms: i64,
    /// Media source of the first clip on the track, if any.
    pub media: Option<String>,
}

/// The full parsed timeline for a composition.
///
/// `PartialEq` only (not `Eq`): it contains `Clip`s with `f64` timing fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineState {
    pub project_id: String,
    pub composition_id: String,
    /// The composition file that was parsed (relative to the project dir).
    pub entry: String,
    pub clips: Vec<Clip>,
    pub tracks: Vec<TimelineTrack>,
    /// Total composition duration in milliseconds.
    pub duration_ms: i64,
    /// Canvas width in pixels (from the first clip, else default).
    pub width: u32,
    /// Canvas height in pixels (from the first clip, else default).
    pub height: u32,
    /// Frames per second (default 30; real value comes from render config).
    pub fps: u32,
}

// --- regexes (compiled once, memoized) -------------------------------------

fn tag_open_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // A tag opening with an attribute section, e.g. `<div class="clip" ...>`.
        // `[^>]*` stops at the first `>`; HyperFrames attribute values do not
        // contain `>`, so this is safe for our purpose.
        Regex::new(r#"<(?<tag>[a-zA-Z][a-zA-Z0-9]*)(?<attrs>[^>]*)?>"#).unwrap()
    })
}

fn data_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // A `data-xxx="value"` / `data-xxx='value'` / `data-xxx=value` pair.
        Regex::new(r#"data-([a-z0-9-]*)\s*=\s*("([^"]*)"|'([^']*)'|([^\s"'>]+))"#).unwrap()
    })
}

fn class_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"class\s*=\s*("([^"]*)"|'([^']*)')"#).unwrap())
}

/// Extract all `data-*` attributes from an attribute string into a map.
fn extract_data_attrs(attrs: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for caps in data_attr_re().captures_iter(attrs) {
        let key = caps.get(1).unwrap().as_str().to_string();
        // Prefer double-quoted, then single-quoted, then unquoted group.
        let value = caps
            .get(3)
            .or_else(|| caps.get(4))
            .or_else(|| caps.get(5))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        map.insert(key, value);
    }
    map
}

/// The raw `class` attribute value, if the tag is quoted.
fn extract_class(attrs: &str) -> Option<String> {
    class_attr_re().captures(attrs).and_then(|c| {
        c.get(2)
            .or_else(|| c.get(3))
            .map(|m| m.as_str().to_string())
    })
}

/// Whether an attribute string marks a clip element.
fn is_clip(attrs: &str) -> bool {
    let has_src = attrs.contains("data-composition-src");
    let cls = extract_class(attrs).unwrap_or_default();
    let has_clip_class = cls.split_whitespace().any(|c| c == "clip");
    has_src || has_clip_class
}

/// Parse a single clip from a tag's attribute string.
fn parse_clip(attrs: &str) -> Option<Clip> {
    let data = extract_data_attrs(attrs);

    // A clip must carry a composition id or a src reference.
    let has_id = data
        .get("composition-id")
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let has_src = data
        .get("composition-src")
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    if !has_id && !has_src {
        return None;
    }

    let num = |k: &str| -> f64 {
        data.get(k)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    let integer = |k: &str| -> i32 { data.get(k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0) };
    let uint = |k: &str| -> Option<u32> { data.get(k).and_then(|v| v.parse::<u32>().ok()) };

    Some(Clip {
        id: data.get("composition-id").cloned().unwrap_or_default(),
        src: data
            .get("composition-src")
            .cloned()
            .filter(|s| !s.is_empty()),
        start_s: num("start"),
        duration_s: num("duration"),
        track_index: integer("track-index"),
        width: uint("width"),
        height: uint("height"),
        variables: data.get("composition-variables").cloned(),
    })
}

/// Parse every clip found in a composition HTML string.
pub fn parse_composition_html(html: &str) -> Vec<Clip> {
    tag_open_re()
        .captures_iter(html)
        .filter(|c| is_clip(c.get(2).unwrap().as_str()))
        .filter_map(|c| parse_clip(c.get(2).unwrap().as_str()))
        .collect()
}

/// Group clips into ordered tracks by `data-track-index`.
///
/// Returns `(tracks, total_duration_ms)`. Clips on the same track are ordered
/// by start time; each track's start/duration span its clips. The total
/// duration is the latest `start + duration` across all clips.
pub fn clips_to_tracks(clips: &[Clip]) -> (Vec<TimelineTrack>, i64) {
    if clips.is_empty() {
        return (Vec::new(), 0);
    }

    // Group by track index, preserving the order of first appearance.
    let mut groups: Vec<(i32, Vec<usize>)> = Vec::new();
    for (i, clip) in clips.iter().enumerate() {
        if let Some(pos) = groups.iter().position(|(idx, _)| *idx == clip.track_index) {
            groups[pos].1.push(i);
        } else {
            groups.push((clip.track_index, vec![i]));
        }
    }

    let mut tracks = Vec::new();
    let mut end_ms = 0i64;

    for (index, indices) in groups {
        let mut ordered = indices;
        ordered.sort_by(|&a, &b| {
            clips[a]
                .start_s
                .partial_cmp(&clips[b].start_s)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let first = &clips[ordered[0]];
        let start_ms = (first.start_s * 1000.0).round() as i64;

        let mut max_end_ms = 0i64;
        for &i in &ordered {
            let end = (clips[i].start_s + clips[i].duration_s) * 1000.0;
            let end_i = end.round() as i64;
            if end_i > max_end_ms {
                max_end_ms = end_i;
            }
        }
        if max_end_ms > end_ms {
            end_ms = max_end_ms;
        }

        let duration_ms = (max_end_ms - start_ms).max(0);
        let media = first
            .src
            .as_ref()
            .and_then(|s| Path::new(s).file_name())
            .map(|n| n.to_string_lossy().to_string());
        let label = clip_label(first);

        tracks.push(TimelineTrack {
            index,
            name: label.clone(),
            label,
            start_ms,
            duration_ms,
            media,
        });
    }

    (tracks, end_ms)
}

/// Derive a human label for a clip from its id or src basename.
fn clip_label(clip: &Clip) -> String {
    if !clip.id.is_empty() {
        return clip.id.clone();
    }
    match &clip.src {
        Some(s) => Path::new(s)
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "clip".to_string()),
        None => "clip".to_string(),
    }
}

/// Default canvas size when a composition declares no explicit dimensions.
const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;
const DEFAULT_FPS: u32 = 30;

/// Common composition file locations, most-specific first.
pub fn composition_candidates(project_dir: &Path, composition_id: &str) -> Vec<PathBuf> {
    vec![
        project_dir.join("composition.html"),
        project_dir.join("index.html"),
        project_dir
            .join("compositions")
            .join(format!("{composition_id}.html")),
        project_dir
            .join("compositions")
            .join(composition_id)
            .join("index.html"),
        project_dir.join(composition_id).join("index.html"),
    ]
}

/// Find the composition file for a project/composition, if it exists on disk.
pub fn discover_composition_file(project_dir: &Path, composition_id: &str) -> Option<PathBuf> {
    composition_candidates(project_dir, composition_id)
        .into_iter()
        .find(|p| p.is_file())
}

/// Parse a composition file (and its referenced sub-compositions) into a
/// `TimelineState`.
///
/// `composition_id` is used only to locate the entry file; the returned state
/// echoes it back. Sub-compositions referenced via `data-composition-src` are
/// resolved relative to the referencing file and parsed recursively up to
/// `max_depth` levels deep, de-duplicated by resolved path.
pub fn load_timeline(project_dir: &Path, composition_id: &str) -> Result<TimelineState> {
    let entry = discover_composition_file(project_dir, composition_id)
        .context("no composition file found in project directory")?;

    let mut clips: Vec<Clip> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    parse_file_into(&entry, project_dir, 0, &mut clips, &mut seen, 4)?;

    let (tracks, duration_ms) = clips_to_tracks(&clips);

    // Canvas size: first non-zero clip dimension, else defaults.
    let width = clips.iter().find_map(|c| c.width).unwrap_or(DEFAULT_WIDTH);
    let height = clips
        .iter()
        .find_map(|c| c.height)
        .unwrap_or(DEFAULT_HEIGHT);

    let rel = entry
        .strip_prefix(project_dir)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| entry.to_string_lossy().to_string());
    let rel = rel.trim_start_matches(['/', '\\']).to_string();

    Ok(TimelineState {
        project_id: String::new(),
        composition_id: composition_id.to_string(),
        entry: rel,
        clips,
        tracks,
        duration_ms,
        width,
        height,
        fps: DEFAULT_FPS,
    })
}

/// Recursively parse a composition file, appending its clips (and those of any
/// referenced sub-compositions) to `clips`.
fn parse_file_into(
    file: &Path,
    base: &Path,
    depth: usize,
    clips: &mut Vec<Clip>,
    seen: &mut std::collections::HashSet<PathBuf>,
    max_depth: usize,
) -> Result<()> {
    let canonical = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    if !seen.insert(canonical.clone()) {
        return Ok(()); // already parsed this file
    }
    if depth >= max_depth {
        return Ok(());
    }

    let html = std::fs::read_to_string(file)
        .with_context(|| format!("reading composition file {}", file.display()))?;

    for clip in parse_composition_html(&html) {
        clips.push(clip.clone());

        // Resolve and recurse into referenced sub-compositions.
        if let Some(src) = &clip.src {
            let parent = file.parent().unwrap_or(base);
            let child = parent.join(src);
            if child.is_file() {
                parse_file_into(&child, base, depth + 1, clips, seen, max_depth)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<!DOCTYPE html>
<html>
  <body>
    <div class="clip"
         data-composition-id="title"
         data-composition-src="blocks/title.html"
         data-start="0"
         data-duration="6"
         data-track-index="0"
         data-width="1280"
         data-height="720">
    </div>
    <div class="block other"
         data-composition-id="scene1"
         data-composition-src="assets/scene1.png"
         data-start="0"
         data-duration="12"
         data-track-index="1"
         data-width="1280"
         data-height="720"></div>
    <div class="clip"
         data-composition-id="scene2"
         data-composition-src="assets/scene2.png"
         data-start="6"
         data-duration="12"
         data-track-index="1"></div>
  </body>
</html>"#;

    #[test]
    fn parses_all_clips_and_keeps_timing() {
        let clips = parse_composition_html(SAMPLE);
        assert_eq!(clips.len(), 3);

        let title = &clips[0];
        assert_eq!(title.id, "title");
        assert_eq!(title.src.as_deref(), Some("blocks/title.html"));
        assert_eq!(title.start_s, 0.0);
        assert_eq!(title.duration_s, 6.0);
        assert_eq!(title.track_index, 0);
        assert_eq!(title.width, Some(1280));
        assert_eq!(title.height, Some(720));
    }

    #[test]
    fn ignores_non_clip_elements() {
        // A plain div with no clip markers must not be parsed.
        let html = r#"<div class="container"><span>hi</span></div>"#;
        assert!(parse_composition_html(html).is_empty());
    }

    #[test]
    fn groups_tracks_by_index_and_orders_by_start() {
        let clips = parse_composition_html(SAMPLE);
        let (tracks, total) = clips_to_tracks(&clips);

        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].index, 0);
        assert_eq!(tracks[0].name, "title");
        assert_eq!(tracks[1].index, 1);
        // Track 1 holds scene1 (t=0..12) then scene2 (t=6..18); spans 0..18.
        assert_eq!(tracks[1].start_ms, 0);
        assert_eq!(tracks[1].duration_ms, 18000);
        assert_eq!(tracks[1].media.as_deref(), Some("scene1.png"));
        // scene2 runs t=6..18, so the composition spans 18s total.
        assert_eq!(total, 18000);
    }

    #[test]
    fn handles_single_quoted_and_unquoted_values() {
        // A clip is detected by `class="clip"` or `data-composition-src` (the
        // HyperFrames contract); the unquoted `data-duration=3` exercises the
        // unquoted-value branch.
        let html = r#"<div class="clip" data-composition-id='q' data-composition-src='q.html' data-start='1.5' data-duration=3 data-track-index='2'></div>"#;
        let clips = parse_composition_html(html);
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].start_s, 1.5);
        assert_eq!(clips[0].duration_s, 3.0);
        assert_eq!(clips[0].track_index, 2);
    }

    #[test]
    fn load_timeline_reads_file_from_disk() {
        let dir = std::env::temp_dir().join(format!("amaara-timeline-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("composition.html");
        std::fs::write(&file, SAMPLE).unwrap();

        let state = load_timeline(&dir, "comp").expect("should parse");
        assert_eq!(state.composition_id, "comp");
        assert_eq!(state.clips.len(), 3);
        assert_eq!(state.tracks.len(), 2);
        assert_eq!(state.width, 1280);
        assert!(state.duration_ms > 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_timeline_reports_missing_dir() {
        let err = load_timeline(Path::new("/definitely/not/here/xyz"), "comp").unwrap_err();
        assert!(err.to_string().contains("no composition file"));
    }

    #[test]
    fn resolves_referenced_sub_compositions_recursively() {
        let dir = std::env::temp_dir().join(format!("amaara-timeline-sub-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("blocks")).unwrap();
        std::fs::write(
            dir.join("composition.html"),
            r#"<div class="clip" data-composition-id="root" data-composition-src="blocks/title.html" data-start="0" data-duration="4" data-track-index="0"></div>"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("blocks/title.html"),
            r#"<div class="clip" data-composition-id="child" data-start="0" data-duration="4" data-track-index="0"></div>"#,
        )
        .unwrap();

        let state = load_timeline(&dir, "comp").expect("should parse");
        // root + its referenced child.
        assert_eq!(state.clips.len(), 2);
        let ids: Vec<&str> = state.clips.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"root") && ids.contains(&"child"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
