# AGENTS.md — AutoShorts project memory

> **Read this first.** This file is the durable handoff for anyone (human or AI agent)
> picking up work in this repo. It records what the project is, how to run and verify it,
> what has already been built, and the non-obvious traps that have already cost time.
>
> Companion file: [`.github/copilot-instructions.md`](.github/copilot-instructions.md) is
> auto-loaded into VS Code Copilot Chat. Keep the two in sync when you change behavior.

---

## 1. What this project is

**AutoShorts** — a desktop app that turns long recordings into vertical 9:16 clips ready
for Instagram Reels / YouTube Shorts. It transcribes the source, asks an LLM to find the
most "viral" moments, scores and ranks them, then renders each one as a captioned vertical
video with burned-in word-level subtitles.

It also has a second, independent workflow (**Clip Mix Studio**, see §6) that takes a
folder of many short clips and stitches them into one polished vertical short with music.

## 2. Stack, toolchain, and where things live

| Piece | Version / path |
|---|---|
| Shell | Tauri 2.5 |
| Frontend | React 19 + TypeScript, bundled by Vite 6 (`127.0.0.1:1420`) |
| Backend | Rust (edition per `src-tauri/Cargo.toml`), `rusqlite` bundled SQLite, `reqwest` (rustls-tls), `dotenvy`, `chrono`, `dirs`, `uuid` |
| Node / npm | v22.17.0 / 10.9.2 |
| Rust | cargo + rustc **1.98.1** |
| MSVC | Visual Studio **2022 Build Tools** + Windows SDK, at `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools` |
| ffmpeg / ffprobe | **9.0.2** (gyan.dev essentials, libx264 + libfreetype), at `C:\Users\USER\.local\ffmpeg\bin` |
| Python | `openai-whisper` + torch, `base.pt` model cached locally |

Frontend is only **two files**: [`src/main.tsx`](src/main.tsx) and [`src/styles.css`](src/styles.css).
There is no `App.tsx` and no separate component directory — every component lives in `main.tsx`.

Source layout:

```
src/main.tsx              React UI (single file, ~3.4k lines)
src/styles.css            All CSS, theme custom properties at the top
src-tauri/src/lib.rs      Tauri commands, app wiring, ffmpeg filter builders
src-tauri/src/db.rs       SQLite schema + DAOs
src-tauri/src/models.rs   Shared serde structs
src-tauri/src/mixer.rs    Clip Mix Studio render engine  <-- newest module
src-tauri/src/llm.rs      LLM provider clients (DeepSeek / OpenAI / etc.)
src-tauri/src/media.rs    ffprobe wrappers
src-tauri/src/transcription.rs  Local Whisper / cloud Deepgram
```

## 3. API keys — where the user puts them

**File: `.env` in the repo root.** It is gitignored (`.gitignore:5`). It is a fully
commented template; the only line active by default is:

```
LLM_PROVIDER=deepseek
```

To enable DeepSeek the user uncomments `#DEEPSEEK_API_KEY=...` and pastes the key — **no
quotes, no spaces**. Restart the app after editing.

> Why keys are left commented out: `environment_status` in `lib.rs` reports a provider as
> configured using `std::env::var(...).is_ok()`, so a present-but-empty variable would
> wrongly show as configured.

Other supported providers (all optional, all in `.env`): Deepgram (transcription),
Anthropic, Gemini, OpenAI, OpenRouter, Groq. Local Whisper is the offline alternative to
Deepgram and is already installed on this machine.

### Known blocker: onboarding cannot be completed with a DeepSeek key alone (Windows)

Three things combine into this trap — do not be surprised by it, and fix it only if asked:

1. `handleCloudSubmit` in `src/main.tsx` hard-requires a Deepgram key.
2. `install_ollama` in `src-tauri/src/lib.rs:156` is macOS-only, so the "Fully Offline"
   path cannot auto-install on Windows.
3. There is no "skip" button on the onboarding screen.

Workarounds for the user:

- **A** — get a free Deepgram key at `https://console.deepgram.com/`.
- **B** — devtools: `localStorage.setItem('autoshorts_onboarded','true')` then reload.
  This works because `has_local_whisper_model` already reports true.

## 4. Running the app

### CRITICAL: cargo and link.exe are not on the inherited PATH

Every cargo/git-bash build command **must** prepend the cargo and ffmpeg bins *and* call
`vcvars64.bat` **in the same `cmd.exe` process**. A `.bat` invoked in one shell call does
not affect a later call.

```powershell
$machine = [Environment]::GetEnvironmentVariable('Path','Machine')
$user    = [Environment]::GetEnvironmentVariable('Path','User')
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:USERPROFILE\.local\ffmpeg\bin;$user;$machine"
& $env:ComSpec /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul && cd /d C:\karti\my_working_dir\autoshorts\src-tauri && cargo test --lib 2>&1'
```

Harmless noise you can ignore in that output:
- `'vswhere.exe' is not recognized ...`
- `Blocking waiting for file lock on build directory`

### Launch

```powershell
npm run tauri:dev
```

Runs Vite (`beforeDevCommand`) plus `cargo run`. For an agent, launch it detached with the
same PATH/vcvars preamble and redirect output to a log.

Scripts (`package.json`):

| Script | Command |
|---|---|
| `dev` | `vite --host 127.0.0.1 --port 1420` |
| `build` | `tsc && vite build` |
| `tauri:dev` | `tauri dev` |
| `tauri:build` | `tauri build` |

### Restarting after a code change

The Tauri window must be closed before a rebuild can replace `target\debug\autoshorts.exe`.
To restart, stop the specific PIDs of the dev tree (npm → `tauri.js dev` → `cargo` →
`autoshorts.exe`, plus the Vite process), confirm port 1420 is free, then relaunch.
Never kill by image name.

## 5. Verifying changes

Smallest useful commands:

```powershell
npm run build                        # tsc + vite, catches all frontend type errors
cargo check --all-targets            # fast; must be warning-free
cargo test --lib                     # unit tests (10 currently, all in mixer.rs + lib.rs)
```

There is **no test runner for the frontend**; `tsc` is the frontend gate. CI is
`.github/workflows/release.yml` (release-only, no lint/clippy step).

### Driving the UI

**A normal browser tab cannot exercise the app.** `http://127.0.0.1:1420` renders, but any
`invoke()` call throws `TypeError: Cannot read properties of undefined (reading 'invoke')`
because there is no Tauri IPC bridge (`window.__TAURI_INTERNALS__`). To validate backend
behavior, write a temporary Rust test (or a small ffmpeg invocation) and delete it
afterwards — that is the established pattern in this repo.

### Verifying a render actually worked

Inspect the produced file with `ffprobe` / `loudnorm` rather than trusting the log line.
Expect `1080x1920`, `h264`, `aac` 48 kHz stereo, `r_frame_rate=30/1`. Integrated loudness
should land near **-14 LUFS** when `loudnorm` is on.

## 6. Feature: Clip Mix Studio

Takes a folder of the user's own short clips and produces one Instagram-ready vertical
short. Entry point in the UI: sidebar → **"Mix clips into a short"**.

**Backend** — [`src-tauri/src/mixer.rs`](src-tauri/src/mixer.rs):

| Symbol | Line | Role |
|---|---|---|
| `MediaEntry` | 38 | serde struct for a scanned clip/track |
| `MixOptions` | 52 | the full render request, `#[serde(rename_all="camelCase")]` |
| `scan_clip_folder` | 188 | recursive scan (8 levels), probes each file |
| `scan_music_folder` | 212 | same, for audio |
| `natural_cmp` | 278 | human ordering — `clip 2` precedes `clip 10` |
| `plan_durations` | 303 | per-clip / total duration planning |
| `normalize_clip` | 402 | conforms every clip to identical geometry + codec + fps + GOP |
| `audio_chain` | 480 | original vs music vs both, plus `loudnorm` |
| `render_with_cuts` | 558 | hard-cut join (concat demuxer, `-c:v copy` w/ libx264 fallback) |
| `render_with_fade` | 618 | `xfade` + `acrossfade` join |
| `render_mix_with_progress` | 722 | **public entry point**, takes an `on_progress` callback |

Two-stage design: every clip is normalized to identical parameters first, then joined. This
makes the join bulletproof and avoids one giant fragile `filter_complex`.

**Frontend** — [`src/main.tsx`](src/main.tsx): `estimateMixDurations` (1462, mirrors
`plan_durations`), `ClipMixStudio` (1483). State `showMixStudio` (185), sidebar button
(~775), workspace branch (~1152). Styles: the `/* Clip Mix Studio */` block at the end of
[`src/styles.css`](src/styles.css).

**Progress** is streamed as the Tauri event `clip-mix-progress` with payload
`{ mixId, message, done }`; the frontend subscribes with `listen` in a `useEffect`.

**Output:** `<Documents>\AutoShorts\Clip Mixes\<slug>\<slug>.mp4` (`mix_output_path`,
`src-tauri/src/lib.rs:802`). Always 1080x1920, 30 fps, H.264 + AAC 48 kHz stereo.

**DB:** `clip_mixes` table (`db.rs` `migrate()`), DAOs `create_clip_mix` / `update_clip_mix`
/ `get_clip_mix` / `list_clip_mixes` / `delete_clip_mix`, row mapper `clip_mix_from_row`.
`update_clip_mix` uses `COALESCE` so passing `None` does not clobber a stored value.
Statuses: `pending` → `rendering` → `done` | `failed`.

## 7. Hard-won gotchas

**ffmpeg**

- **`acrossfade` has exactly ONE output.** Passing two output labels fails with
  `More output link labels specified for filter 'acrossfade' than it has outputs: 2 > 1`.
- **A dangling label aborts the entire `-filter_complex` render.** The error
  `Filter 'acrossfade:default' has output 1 (acat) unconnected` means *the graph label was
  never consumed downstream*, not that the filter has a second output. This was a real bug
  here: `render_with_fade` always emitted an `[acat]` label, but when `keep_original_audio`
  was false nothing consumed it, so crossfade mode silently degraded to hard cuts. Fixed by
  only building the audio fan-in when original audio is actually kept (`orig_label` is `""`
  otherwise, and `audio_chain` only references it when `keep_original_audio` is true).
  **Whenever you emit a label, guarantee a consumer.**
- `-stream_loop -1` must come **before** `-i`.
- Silent audio track: `-f lavfi -i anullsrc=channel_layout=stereo:sample_rate=48000`.
  `normalize_clip` prepends this when a source clip has no audio stream (shifting the video
  input index to 1), which is what guarantees `{index}:a` always exists for the fade path.
- `amix` needs `normalize=0` or it halves the volume.
- For a blurred background, `boxblur` on a quarter-res downscale then upscaling is far
  faster than blurring at full 1080x1920.
- Use fixed-GOP encode settings (`-g 60 -keyint_min 60 -sc_threshold 0`) so downstream
  joins and copies behave.

**Windows / PowerShell**

- No `&&`, `||`, `?:`, `??=`, `?.` in this PowerShell. Use `;` and explicit `if ($?) { }`.
- `cd /d ... && ...` does not parse — use `Set-Location ...; ...` or `& $env:ComSpec /c`.
- There is **no heredoc**. For inline Python use a single-quoted here-string
  (`@'` on its own line, script, column-0 `'@ | python -`) or `python -c "..."`.
- Building strings inside an array literal can silently produce an empty value. Prefer
  `'{0}=...' -f $x` or a hashtable.
- To write files without a BOM: `[System.IO.File]::WriteAllText($p, $s, (New-Object System.Text.UTF8Encoding $false))`.

**Repo noise — do not "fix" these**

- `src-tauri/Cargo.toml` shows as modified in `git status` but `git diff` is **empty**.
  That is a CRLF/LF line-ending artifact.
- `src-tauri/gen/schemas/*.json` are auto-regenerated by `tauri dev` and will show as
  modified.

## 8. Product constraints (do not violate)

- **Never auto-download "trending" music from Instagram or TikTok.** That audio is
  licensed commercial music; scraping it is copyright infringement that gets users'
  accounts struck. The Clip Mix feature explicitly points the user at their **own** music
  folder or a royalty-free library, and both the README and the UI state this. "Trending
  ready" is delivered as *production quality* instead: fades, music balanced against voice,
  and -14 LUFS loudness normalization.
- Keep the existing secret-handling pattern: no credentials in source, only `.env`.

## 9. Change log

### 2026-10 — Clip Mix Studio added

- New `src-tauri/src/mixer.rs` (scanning, natural sort, duration planning, per-clip
  normalization, cut/crossfade joins, music mixing, loudness normalization) plus its unit
  tests. `render_mix_with_progress` exposes a live progress callback.
- `ClipMix` model, `clip_mixes` table, full CRUD DAO.
- Six Tauri commands registered in `generate_handler!`.
- `ClipMixStudio` React component + `estimateMixDurations` helper + ~370 lines of
  `mix-*` CSS + sidebar entry + workspace branch.
- Fixed the crossfade dangling-label bug described in §7.
- Verified: `npm run build` clean; `cargo check --all-targets` warning-free; `cargo test
  --lib` 10/10; end-to-end render against real ffmpeg fixtures (mixed resolutions including
  a silent clip) measured at 1080x1920 / 30 fps / -13.2 and -14.1 LUFS; SQLite round-trip of
  a `ClipMix` verified including `COALESCE` non-clobber behavior.
- README updated with a Clip Mix Studio section and the music-licensing callout.

### Earlier — core pipeline setup

- Windows toolchain brought up: Rust 1.98.1, MSVC 2022 Build Tools + SDK, ffmpeg 9.0.2,
  yt-dlp, openai-whisper + torch.
- `.env` created as the documented key location.
- Local Whisper transcription validated end-to-end against synthesized speech
  (verbatim-correct transcript with word timestamps).
- Confirmed a suspected hardcoded secret in `llm.rs:61` was a **false positive** — it is a
  `format!("Bearer {}", api_key)` redaction artifact, not a literal.
