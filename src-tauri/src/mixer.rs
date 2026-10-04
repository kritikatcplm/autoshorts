//! Clip Mix: turn a folder of short clips into one finished vertical short
//! with music, transitions and platform-ready loudness.

use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::media;

const VIDEO_EXTS: &[&str] = &[
    "mp4", "mov", "m4v", "avi", "mkv", "webm", "mpg", "mpeg", "wmv", "flv", "3gp",
];
const AUDIO_EXTS: &[&str] = &[
    "mp3", "wav", "m4a", "aac", "flac", "ogg", "oga", "opus", "wma", "aiff",
];

const MAX_WALK_DEPTH: usize = 8;

fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

fn has_extension_in(path: &Path, allowed: &[&str]) -> bool {
    extension_of(path)
        .map(|ext| allowed.contains(&ext.as_str()))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaEntry {
    pub path: String,
    pub file_name: String,
    pub relative_dir: Option<String>,
    pub duration_sec: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub has_video: bool,
    pub has_audio: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixOptions {
    pub clips: Vec<String>,
    pub music: Option<String>,
    #[serde(default = "default_music_volume")]
    pub music_volume: f64,
    #[serde(default = "default_original_volume")]
    pub original_volume: f64,
    #[serde(default = "default_true")]
    pub keep_original_audio: bool,
    #[serde(default)]
    pub per_clip_sec: Option<f64>,
    #[serde(default)]
    pub max_total_sec: Option<f64>,
    #[serde(default = "default_fit")]
    pub fit: String,
    #[serde(default = "default_transition")]
    pub transition: String,
    #[serde(default = "default_transition_sec")]
    pub transition_sec: f64,
    #[serde(default = "default_width")]
    pub target_w: i64,
    #[serde(default = "default_height")]
    pub target_h: i64,
    #[serde(default = "default_fps")]
    pub fps: i64,
    #[serde(default = "default_true")]
    pub loudnorm: bool,
}

fn default_music_volume() -> f64 {
    0.35
}
fn default_original_volume() -> f64 {
    1.0
}
fn default_true() -> bool {
    true
}
fn default_fit() -> String {
    "crop".to_string()
}
fn default_transition() -> String {
    "cut".to_string()
}
fn default_transition_sec() -> f64 {
    0.5
}
fn default_width() -> i64 {
    1080
}
fn default_height() -> i64 {
    1920
}
fn default_fps() -> i64 {
    30
}

#[derive(Debug, Clone)]
struct PlannedClip {
    path: PathBuf,
    duration: f64,
    source_duration: f64,
}

// ---------------------------------------------------------------------------
// Folder scanning
// ---------------------------------------------------------------------------

fn collect_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<()> {
    if depth > MAX_WALK_DEPTH {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("reading folder {}", dir.display()))?;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        // `file_type()` does not follow symlinks, which also prevents loops.
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        let path = entry.path();
        if file_type.is_dir() {
            collect_files(&path, depth + 1, out)?;
        } else if file_type.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn to_entry(path: &Path, root: &Path, probe_media: bool) -> MediaEntry {
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let relative_dir = path
        .parent()
        .and_then(|parent| parent.strip_prefix(root).ok())
        .map(|rel| rel.to_string_lossy().to_string())
        .filter(|rel| !rel.is_empty());

    let (duration_sec, width, height, has_video, has_audio) = if probe_media {
        match media::probe_media(&path.to_string_lossy()) {
            Ok(probe) => (
                probe.duration_sec,
                probe.width,
                probe.height,
                probe.has_video,
                probe.audio_codec.is_some(),
            ),
            Err(_) => (None, None, None, false, false),
        }
    } else {
        (None, None, None, false, false)
    };

    MediaEntry {
        path: path.to_string_lossy().to_string(),
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        relative_dir,
        duration_sec,
        width,
        height,
        has_video,
        has_audio,
        size_bytes,
    }
}

/// List video files in a folder (recursively), ordered the way a human expects
/// so "clip2" sorts before "clip10".
pub fn scan_clip_folder(folder: &str) -> Result<Vec<MediaEntry>> {
    let root = PathBuf::from(folder);
    if !root.is_dir() {
        bail!("'{folder}' is not a folder.");
    }

    let mut files = Vec::new();
    collect_files(&root, 0, &mut files)?;

    let mut entries: Vec<MediaEntry> = files
        .iter()
        .filter(|path| has_extension_in(path, VIDEO_EXTS))
        .map(|path| to_entry(path, &root, true))
        .collect();

    // Drop anything ffprobe could not read a video stream out of.
    entries.retain(|entry| entry.has_video);
    entries.sort_by(|a, b| {
        natural_cmp(&a.file_name, &b.file_name)
            .then_with(|| natural_cmp(&a.path, &b.path))
    });
    Ok(entries)
}

pub fn scan_music_folder(folder: &str) -> Result<Vec<MediaEntry>> {
    let root = PathBuf::from(folder);
    if !root.is_dir() {
        bail!("'{folder}' is not a folder.");
    }

    let mut files = Vec::new();
    collect_files(&root, 0, &mut files)?;

    let mut entries: Vec<MediaEntry> = files
        .iter()
        .filter(|path| has_extension_in(path, AUDIO_EXTS))
        .map(|path| to_entry(path, &root, true))
        .collect();

    entries.sort_by(|a, b| {
        natural_cmp(&a.file_name, &b.file_name)
            .then_with(|| natural_cmp(&a.path, &b.path))
    });
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Natural (human) string ordering
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Num(u128),
    Text(String),
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_digits = false;

    let flush = |current: &mut String, in_digits: bool, tokens: &mut Vec<Token>| {
        if current.is_empty() {
            return;
        }
        if in_digits {
            let value = current.parse::<u128>().unwrap_or(u128::MAX);
            tokens.push(Token::Num(value));
        } else {
            tokens.push(Token::Text(current.clone()));
        }
        current.clear();
    };

    for ch in input.chars() {
        let is_digit = ch.is_ascii_digit();
        if !current.is_empty() && is_digit != in_digits {
            flush(&mut current, in_digits, &mut tokens);
        }
        in_digits = is_digit;
        if is_digit {
            current.push(ch);
        } else {
            current.extend(ch.to_lowercase());
        }
    }
    flush(&mut current, in_digits, &mut tokens);
    tokens
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    let left = tokenize(a);
    let right = tokenize(b);

    for (l, r) in left.iter().zip(right.iter()) {
        let ordering = match (l, r) {
            (Token::Num(l), Token::Num(r)) => l.cmp(r),
            (Token::Text(l), Token::Text(r)) => l.cmp(r),
            // Numbers sort ahead of text so "clip-02" precedes "clip-b".
            (Token::Num(_), Token::Text(_)) => Ordering::Less,
            (Token::Text(_), Token::Num(_)) => Ordering::Greater,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// Trim each clip to at most `per_clip_sec` and stop once `max_total_sec` is
/// reached, shortening the final clip if needed.
fn plan_durations(source: &[f64], per_clip: Option<f64>, max_total: Option<f64>) -> Vec<f64> {
    let mut planned = Vec::new();
    let mut total = 0.0;

    for &raw in source {
        let mut duration = raw.max(0.05);
        if let Some(limit) = per_clip {
            duration = duration.min(limit.max(0.05));
        }
        if let Some(max_total) = max_total {
            if total >= max_total {
                break;
            }
            duration = duration.min(max_total - total);
        }
        if duration < 0.05 {
            break;
        }
        total += duration;
        planned.push(duration);
    }
    planned
}

fn build_plan(opts: &MixOptions) -> Result<Vec<PlannedClip>> {
    if opts.clips.is_empty() {
        bail!("Add at least one clip to build a short.");
    }

    let mut raw = Vec::new();
    for clip in &opts.clips {
        let path = PathBuf::from(clip);
        if !path.is_file() {
            bail!("Clip not found: {clip}");
        }
        let probe = media::probe_media(clip)
            .with_context(|| format!("could not read clip {clip}"))?;
        if !probe.has_video {
            bail!("{} has no video stream.", path.display());
        }
        raw.push(probe.duration_sec.unwrap_or(0.0));
    }

    let durations = plan_durations(&raw, opts.per_clip_sec, opts.max_total_sec);
    if durations.is_empty() {
        bail!("The clips are too short to build a short.");
    }

    Ok(opts
        .clips
        .iter()
        .zip(durations)
        .zip(raw)
        .map(|((clip, duration), source_duration)| PlannedClip {
            path: PathBuf::from(clip),
            duration,
            source_duration,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// ffmpeg helpers
// ---------------------------------------------------------------------------

fn push_log(log: &mut Vec<String>, on_progress: &mut impl FnMut(&str), message: String) {
    on_progress(&message);
    log.push(message);
}

fn stderr_tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.trim().lines().collect();
    let start = lines.len().saturating_sub(12);
    lines[start..].join("\n")
}

fn run_ffmpeg(cmd: &mut Command, what: &str) -> Result<()> {
    let output = cmd
        .output()
        .with_context(|| format!("could not start ffmpeg for {what}"))?;
    if !output.status.success() {
        bail!("{what} failed:\n{}", stderr_tail(&String::from_utf8_lossy(&output.stderr)));
    }
    Ok(())
}

fn ffmpeg_available() -> Result<()> {
    if media::command_exists("ffmpeg") {
        Ok(())
    } else {
        Err(anyhow!("ffmpeg is not installed or not available on PATH"))
    }
}

// ---------------------------------------------------------------------------
// Stage 1: normalise every clip to identical geometry/codec so they can be
// joined without artefacts.
// ---------------------------------------------------------------------------

fn normalize_clip(
    clip: &PlannedClip,
    opts: &MixOptions,
    output: &Path,
) -> Result<()> {
    let probe = media::probe_media(&clip.path.to_string_lossy())
        .with_context(|| format!("probing {}", clip.path.display()))?;
    let has_audio = probe.audio_codec.is_some();

    let width = opts.target_w.max(64);
    let height = opts.target_h.max(64);
    let fps = opts.fps.clamp(1, 120);
    let tail = format!("fps={fps},setsar=1,format=yuv420p");

    // When the source has no audio we prepend an infinite silent source so the
    // pipeline downstream always sees a uniform audio stream.
    let video_input_index = if has_audio { 0 } else { 1 };

    let filter = match opts.fit.as_str() {
        "blur" => {
            let bg_w = (width / 4).max(32);
            let bg_h = (height / 4).max(32);
            format!(
                "[{i}:v]split=2[bga][fga];\
                 [bga]scale={bw}:{bh}:force_original_aspect_ratio=increase,crop={bw}:{bh},boxblur=20:2,scale={w}:{h},eq=brightness=-0.06[bgb];\
                 [fga]scale={w}:{h}:force_original_aspect_ratio=decrease[fgb];\
                 [bgb][fgb]overlay=(W-w)/2:(H-h)/2,{tail}[v]",
                i = video_input_index,
                bw = bg_w,
                bh = bg_h,
                w = width,
                h = height,
                tail = tail
            )
        }
        _ => format!(
            "[{i}:v]scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},{tail}[v]",
            i = video_input_index,
            w = width,
            h = height,
            tail = tail
        ),
    };

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y");
    if !has_audio {
        cmd.args([
            "-f",
            "lavfi",
            "-i",
            "anullsrc=channel_layout=stereo:sample_rate=48000",
        ]);
    }
    cmd.arg("-i").arg(&clip.path);
    cmd.args(["-filter_complex", &filter]);
    cmd.args(["-map", "[v]", "-map", "0:a"]);
    cmd.args(["-t", &format!("{:.3}", clip.duration)]);
    cmd.args([
        "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p",
    ]);
    // Fixed GOP so the segments can later be joined with stream copy.
    cmd.args(["-g", "60", "-keyint_min", "60", "-sc_threshold", "0"]);
    cmd.args(["-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2"]);
    cmd.arg(output);

    run_ffmpeg(
        &mut cmd,
        &format!("normalising {}", clip.path.display()),
    )
}

// ---------------------------------------------------------------------------
// Stage 2: audio graph
// ---------------------------------------------------------------------------

/// Build the audio filter chain. `orig_label` is a filterpad/stream label for
/// the joined clips, `music_label` the optional music stream label.
fn audio_chain(
    opts: &MixOptions,
    orig_label: &str,
    music_label: Option<&str>,
    final_dur: f64,
) -> (Vec<String>, String) {
    let mut parts = Vec::new();
    let out;

    if let Some(music) = music_label {
        let fade_out_start = (final_dur - 1.5).max(0.1);
        parts.push(format!(
            "[{music}]volume={vol:.3},afade=t=in:st=0:d=1.2,afade=t=out:st={fade:.3}:d=1.5[mus]",
            music = music,
            vol = opts.music_volume.clamp(0.0, 2.0),
            fade = fade_out_start
        ));

        if opts.keep_original_audio {
            parts.push(format!(
                "[{orig}]volume={vol:.3}[origaud]",
                orig = orig_label,
                vol = opts.original_volume.clamp(0.0, 2.0)
            ));
            parts.push(
                "[origaud][mus]amix=inputs=2:duration=first:normalize=0[mix]".to_string(),
            );
            out = "mix".to_string();
        } else {
            out = "mus".to_string();
        }
    } else if opts.keep_original_audio {
        parts.push(format!(
            "[{orig}]volume={vol:.3}[origaud]",
            orig = orig_label,
            vol = opts.original_volume.clamp(0.0, 2.0)
        ));
        out = "origaud".to_string();
    } else {
        parts.push(format!(
            "anullsrc=channel_layout=stereo:sample_rate=48000,atrim=0:{dur:.3}[sil]",
            dur = final_dur.max(0.1)
        ));
        out = "sil".to_string();
    }

    if opts.loudnorm {
        // Instagram / YouTube target loudness.
        parts.push(format!("[{out}]loudnorm=I=-14:TP=-1:LRA=11[aout]"));
        return (parts, "aout".to_string());
    }

    (parts, out)
}

fn music_input_args(cmd: &mut Command, music: Option<&Path>) {
    if let Some(music) = music {
        cmd.args(["-stream_loop", "-1", "-i"]).arg(music);
    }
}

// ---------------------------------------------------------------------------
// Stage 2a: hard cuts via the concat demuxer (stream copy where possible)
// ---------------------------------------------------------------------------

fn write_concat_list(inputs: &[PathBuf], list_path: &Path) -> Result<()> {
    let mut text = String::new();
    for path in inputs {
        let escaped = path
            .to_string_lossy()
            .replace('\\', "/")
            .replace('\'', "'\\''");
        text.push_str(&format!("file '{escaped}'\n"));
    }
    std::fs::write(list_path, text).context("writing ffmpeg concat list")?;
    Ok(())
}

fn render_with_cuts(
    inputs: &[PathBuf],
    opts: &MixOptions,
    music: Option<&Path>,
    final_dur: f64,
    work_dir: &Path,
    output: &Path,
) -> Result<()> {
    let list_path = work_dir.join("concat.txt");
    write_concat_list(inputs, &list_path)?;

    let music_label = music.map(|_| "1:a");
    let (parts, out_label) = audio_chain(opts, "0:a", music_label, final_dur);
    let graph = parts.join(";");

    // First attempt copies the already-encoded video; if the segments are not
    // compatible ffmpeg fails and we retry with a re-encode.
    for copy_video in [true, false] {
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y");
        cmd.args(["-f", "concat", "-safe", "0", "-i"]).arg(&list_path);
        music_input_args(&mut cmd, music);
        cmd.args(["-filter_complex", &graph]);
        cmd.args(["-map", "0:v"]);
        cmd.args(["-map", &format!("[{out_label}]")]);

        if copy_video {
            cmd.args(["-c:v", "copy"]);
        } else {
            cmd.args([
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p",
            ]);
        }

        cmd.args(["-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2"]);
        if music.is_some() {
            cmd.args(["-t", &format!("{:.3}", final_dur + 0.05)]);
        }
        cmd.args(["-movflags", "+faststart"]);
        cmd.arg(output);

        match run_ffmpeg(&mut cmd, "joining clips") {
            Ok(()) => return Ok(()),
            Err(error) => {
                if copy_video {
                    let _ = std::fs::remove_file(output);
                    continue;
                }
                return Err(error);
            }
        }
    }

    Err(anyhow!("Could not join the clips into a single file."))
}

// ---------------------------------------------------------------------------
// Stage 2b: crossfade transitions via xfade/acrossfade
// ---------------------------------------------------------------------------

fn render_with_fade(
    inputs: &[PathBuf],
    opts: &MixOptions,
    music: Option<&Path>,
    durations: &[f64],
    output: &Path,
) -> Result<()> {
    let count = inputs.len();
    if count < 2 {
        bail!("crossfade needs at least two clips");
    }

    let overlap = opts.transition_sec.clamp(0.1, 1.5);
    // A crossfade consumes `overlap` seconds per join.
    if durations
        .iter()
        .take(count - 1)
        .any(|duration| *duration <= overlap)
    {
        bail!("a clip is shorter than the crossfade length");
    }

    let mut parts = Vec::new();
    let mut video_label = "0:v".to_string();
    let mut cumulative = durations[0];

    for index in 1..count {
        let offset = (cumulative - overlap).max(0.0);
        let is_last = index == count - 1;
        let label = if is_last {
            "vcat".to_string()
        } else {
            format!("vx{index}")
        };
        parts.push(format!(
            "[{prev}][{index}:v]xfade=transition=fade:duration={overlap:.3}:offset={offset:.3}[{label}]",
            prev = video_label,
            overlap = overlap,
            offset = offset,
            label = label
        ));
        cumulative = cumulative + durations[index] - overlap;
        video_label = label;
    }

    let mut audio_label = "0:a".to_string();
    let mut orig_label = String::new();
    // The fan-in is only worth building when the clips' own audio reaches the output;
    // otherwise its final label dangles and ffmpeg rejects the whole filtergraph.
    if opts.keep_original_audio {
        for index in 1..count {
            let is_last = index == count - 1;
            let label = if is_last {
                "acat".to_string()
            } else {
                format!("ax{index}")
            };
            parts.push(format!(
                "[{prev}][{index}:a]acrossfade=d={overlap:.3}:c1=tri:c2=tri[{label}]",
                prev = audio_label,
                overlap = overlap,
                label = label
            ));
            audio_label = label;
        }
        orig_label = "acat".to_string();
    }

    let final_dur = cumulative;
    let music_label = music.map(|_| format!("{count}:a"));
    let (audio_parts, out_label) =
        audio_chain(opts, &orig_label, music_label.as_deref(), final_dur);
    parts.extend(audio_parts);

    let graph = parts.join(";");

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y");
    for input in inputs {
        cmd.arg("-i").arg(input);
    }
    music_input_args(&mut cmd, music);
    cmd.args(["-filter_complex", &graph]);
    cmd.args(["-map", "[vcat]"]);
    cmd.args(["-map", &format!("[{out_label}]")]);
    cmd.args([
        "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p",
    ]);
    cmd.args(["-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2"]);
    if music.is_some() {
        cmd.args(["-t", &format!("{:.3}", final_dur + 0.05)]);
    }
    cmd.args(["-movflags", "+faststart"]);
    cmd.arg(output);

    run_ffmpeg(&mut cmd, "applying crossfades")
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Render a clip mix, forwarding each human-readable step to `on_progress` as it
/// happens so the UI can show live progress during long renders.
pub fn render_mix_with_progress(
    opts: &MixOptions,
    work_dir: &Path,
    output: &Path,
    mut on_progress: impl FnMut(&str),
) -> Result<Vec<String>> {
    ffmpeg_available()?;

    if opts.clips.len() < 1 {
        bail!("Add at least one clip to build a short.");
    }
    if let Some(music) = &opts.music {
        if !Path::new(music).is_file() {
            bail!("Music file not found: {music}");
        }
    }

    std::fs::create_dir_all(work_dir).context("creating render workspace")?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).context("creating output folder")?;
    }

    let mut log = Vec::new();
    let plan = build_plan(opts)?;
    push_log(&mut log, &mut on_progress, format!(
        "Preparing {} clip(s) for a {:.0}x{:.0} {:.0}fps short.",
        plan.len(),
        opts.target_w,
        opts.target_h,
        opts.fps
    ));

    // ---- Stage 1 -----------------------------------------------------------
    let mut normalized = Vec::new();
    for (index, clip) in plan.iter().enumerate() {
        let target = work_dir.join(format!("norm-{index:03}.mp4"));
        normalize_clip(clip, opts, &target)?;
        let name = clip
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| clip.path.to_string_lossy().to_string());
        push_log(&mut log, &mut on_progress, format!(
            "Normalised {name} -> {:.2}s of {:.2}s",
            clip.duration, clip.source_duration
        ));
        normalized.push(target);
    }

    // Re-probe so joins line up exactly with what ffmpeg actually produced.
    let durations: Vec<f64> = normalized
        .iter()
        .enumerate()
        .map(|(index, path)| {
            media::probe_media(&path.to_string_lossy())
                .ok()
                .and_then(|probe| probe.duration_sec)
                .unwrap_or(plan[index].duration)
        })
        .collect();

    let music_path = opts.music.as_deref().map(PathBuf::from);
    let music = music_path.as_deref();

    let cut_duration: f64 = durations.iter().sum();

    // ---- Stage 2 -----------------------------------------------------------
    if opts.transition == "fade" && normalized.len() >= 2 {
        push_log(
            &mut log,
            &mut on_progress,
            format!("Crossfading {} clips...", normalized.len()),
        );
    } else {
        push_log(
            &mut log,
            &mut on_progress,
            format!("Joining {} clips...", normalized.len()),
        );
    }

    let fade_attempt = if opts.transition == "fade" && normalized.len() >= 2 {
        Some(render_with_fade(
            &normalized,
            opts,
            music,
            &durations,
            output,
        ))
    } else {
        None
    };

    match fade_attempt {
        Some(Ok(())) => {
            let overlap = opts.transition_sec.clamp(0.1, 1.5);
            let total = cut_duration - overlap * (normalized.len() - 1) as f64;
            push_log(&mut log, &mut on_progress, format!(
                "Joined {} clips with crossfades ({total:.2}s).",
                normalized.len()
            ));
        }
        Some(Err(error)) => {
            push_log(&mut log, &mut on_progress, format!(
                "Crossfade failed ({error}); falling back to hard cuts."
            ));
            let _ = std::fs::remove_file(output);
            render_with_cuts(&normalized, opts, music, cut_duration, work_dir, output)?;
            push_log(&mut log, &mut on_progress, format!(
                "Joined {} clips with hard cuts ({cut_duration:.2}s).",
                normalized.len()
            ));
        }
        None => {
            render_with_cuts(&normalized, opts, music, cut_duration, work_dir, output)?;
            push_log(&mut log, &mut on_progress, format!(
                "Joined {} clips with hard cuts ({cut_duration:.2}s).",
                normalized.len()
            ));
        }
    }

    if let Some(music) = &opts.music {
        let name = Path::new(music)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        push_log(&mut log, &mut on_progress, format!(
            "Mixed in {name} at {:.0}% with 1.2s fade-in and 1.5s fade-out.",
            opts.music_volume * 100.0
        ));
    }
    if opts.loudnorm {
        push_log(&mut log, &mut on_progress, "Normalised loudness to -14 LUFS (Instagram / YouTube target).".to_string());
    }

    // ---- Clean up intermediates -------------------------------------------
    for path in &normalized {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::remove_file(work_dir.join("concat.txt"));

    Ok(log)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_ordering_handles_numbers() {
        assert_eq!(natural_cmp("clip2.mp4", "clip10.mp4"), Ordering::Less);
        assert_eq!(natural_cmp("clip10.mp4", "clip2.mp4"), Ordering::Greater);
        assert_eq!(natural_cmp("clip02.mp4", "clip2.mp4"), Ordering::Equal);
    }

    #[test]
    fn natural_ordering_is_case_insensitive() {
        assert_eq!(natural_cmp("ClipA.mp4", "clipa.mp4"), Ordering::Equal);
    }

    #[test]
    fn sorting_clips_like_a_human() {
        let mut names = vec!["b.mp4", "a10.mp4", "a2.mp4", "a1.mp4"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, vec!["a1.mp4", "a2.mp4", "a10.mp4", "b.mp4"]);
    }

    #[test]
    fn plan_trims_each_clip() {
        let planned = plan_durations(&[10.0, 10.0], Some(2.0), None);
        assert_eq!(planned, vec![2.0, 2.0]);
    }

    #[test]
    fn plan_stops_at_max_total() {
        let planned = plan_durations(&[5.0, 5.0, 5.0], None, Some(12.0));
        assert_eq!(planned, vec![5.0, 5.0, 2.0]);
    }

    #[test]
    fn plan_keeps_everything_without_limits() {
        let planned = plan_durations(&[1.5, 2.5], None, None);
        assert_eq!(planned, vec![1.5, 2.5]);
    }

    #[test]
    fn plan_ignores_empty_input() {
        assert!(plan_durations(&[], Some(1.0), None).is_empty());
    }

    #[test]
    fn audio_chain_mixes_music_and_original() {
        let opts = MixOptions {
            clips: vec!["a.mp4".into()],
            music: Some("song.mp3".into()),
            music_volume: 0.3,
            original_volume: 1.0,
            keep_original_audio: true,
            per_clip_sec: None,
            max_total_sec: None,
            fit: default_fit(),
            transition: default_transition(),
            transition_sec: default_transition_sec(),
            target_w: default_width(),
            target_h: default_height(),
            fps: default_fps(),
            loudnorm: true,
        };

        let (parts, label) = audio_chain(&opts, "0:a", Some("1:a"), 20.0);
        assert_eq!(label, "aout");
        assert!(parts.iter().any(|p| p.contains("amix")));
        assert!(parts.iter().any(|p| p.contains("loudnorm=I=-14")));
        assert!(parts.iter().any(|p| p.contains("afade=t=out")));
    }

    #[test]
    fn audio_chain_can_replace_original_audio() {
        let opts = MixOptions {
            clips: vec!["a.mp4".into()],
            music: Some("song.mp3".into()),
            music_volume: 0.9,
            original_volume: 1.0,
            keep_original_audio: false,
            per_clip_sec: None,
            max_total_sec: None,
            fit: default_fit(),
            transition: default_transition(),
            transition_sec: default_transition_sec(),
            target_w: default_width(),
            target_h: default_height(),
            fps: default_fps(),
            loudnorm: true,
        };

        let (parts, label) = audio_chain(&opts, "0:a", Some("1:a"), 20.0);
        assert_eq!(label, "aout");
        assert!(!parts.iter().any(|p| p.contains("amix")));
    }
}
