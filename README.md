⭐ [Support AutoShorts](#support)

# AutoShorts

AutoShorts is a local-first desktop application for turning long-form video or audio recordings into high-impact, vertical short-form clip candidates (9:16 portrait) with AI-powered viral moment ranking.

This repository implements the desktop app foundation using **Tauri 2 + React + TSX + Rust + SQLite**.

<img width="1397" height="918" alt="Screenshot 2026-06-22 at 4 10 13 PM" src="https://github.com/user-attachments/assets/3a58ff60-6d9b-46fc-81e8-2778c96aba62" />

---

## Key Features

- **Dynamic Multi-LLM Support**: Supports both **DeepSeek** (default) and **Claude** (anthropic) for viral moment detection and hooks analysis.
- **Automated Pipeline**: Imports media, extracts audio, transcribes using Deepgram, and automatically analyzes and ranks moments in a single automated chain.
- **Local SQLite Storage**: Saves transcripts, candidates, custom names, and rendering data locally.
- **Native Project Manager**: Create, open, rename, and delete projects from the dashboard.
- **Portrait Auto-Cropping**: Automatically center-crops landscape videos to vertical H.264 portrait clips using native `ffmpeg` integration.
- **Clip Mix Studio**: Point at a folder of your own clips and it stitches them into a single 1080x1920 short — reframed, crossfaded or hard-cut, layered with a music bed, and loudness-normalised to the -14 LUFS target that Instagram Reels and YouTube Shorts expect.
- **Key Warnings**: Built-in visual warnings that identify missing environment variables and prompt you directly in the UI.

---

## Prerequisites

To run the application, **FFmpeg & FFprobe** must be installed and available on your system `PATH` to handle cropping, audio extraction, and dynamic captions:

* **macOS**: Install using Homebrew:
  ```bash
  brew install ffmpeg
  ```
  *Note: To ensure full captions rendering support, if standard Homebrew FFmpeg lacks drawtext/subtitles filters, tap and install the `homebrew-ffmpeg` formula:*
  ```bash
  brew tap homebrew-ffmpeg/ffmpeg
  brew install homebrew-ffmpeg/ffmpeg/ffmpeg
  ```
* **Windows**: Install using Winget (in PowerShell):
  ```powershell
  winget install Gyan.FFmpeg
  ```
  *(Or download the release build from [gyan.dev](https://www.gyan.dev/ffmpeg/builds/) and add it to your system PATH environment variables).*
* **Linux**: Install via your native package manager:
  ```bash
  sudo apt install ffmpeg      # Debian/Ubuntu
  sudo pacman -S ffmpeg        # Arch Linux
  sudo dnf install ffmpeg      # Fedora
  ```

---

## Installation Guide (For Users)

Download the correct package matching your system from the latest [GitHub Releases](https://github.com/JayWebtech/autoshorts/releases/tag/autoshorts).

### 🖥️ macOS Installation
1. **Download**:
   * **Apple Silicon (M1/M2/M3)**: Select the `aarch64.dmg` package.
   * **Intel Mac**: Select the `x64.dmg` package.
2. **Install**: Double-click the `.dmg` file and drag **AutoShorts** to your **Applications** folder.
3. **Bypass Gatekeeper** (For unsigned local builds):
   * Right-click `AutoShorts.app` in Finder, select **Open**, and click **Open** in the warning dialog.
   * *Alternatively*, run this command in Terminal:
     ```bash
     xattr -cr /Applications/AutoShorts.app
     ```

### 🪟 Windows Installation
1. **Download**: Select the `.msi` (installer) or `.exe` (portable executable) package.
2. **Install**: Double-click the `.msi` file to run the setup wizard.
3. **SmartScreen Bypass**: Since the package is self-signed, Windows SmartScreen may show a warning. Click **"More Info"** in the window and choose **"Run anyway"**.

### 🐧 Linux Installation
1. **Download**: Select the `.deb` (Debian/Ubuntu) or `.AppImage` (universal portable binary).
2. **Install `.deb`**:
   ```bash
   sudo dpkg -i autoshorts_*.deb
   ```
3. **Run `.AppImage`**:
   Make it executable and launch it:
   ```bash
   chmod +x autoshorts_*.AppImage
   ./autoshorts_*.AppImage
   ```

### 🚀 First-Launch Onboarding & AI Configuration

When you first launch the application, you will be greeted by an **Onboarding Wizard** that lets you choose your preferred workflow:

#### Option A: Fully Offline (Ollama + Whisper)
1. **Ollama Setup**: Select a local model card (`llama3.2 3B`, `qwen2.5 3B`, or `qwen2.5 7B`). The application will check if Ollama is running and automatically pull the model weights, showing a downloader progress bar.
2. **Local Whisper**: Follow the prompt instructions to verify Python is installed and run `pip3 install openai-whisper` to enable fully offline transcription.

#### Option B: Cloud API Keys
1. Enter your API credentials for:
   * **Deepgram**: For fast, accurate cloud transcription.
   * **DeepSeek**: (Highly Recommended) For cheap, high-quality cloud moment detection.
   * **Claude**: For premium copywriting, hooks, and moment detection.
2. Click **Save & Start** to immediately load the dashboard.

---

### ⚙️ Modifying Settings & Resetting Onboarding

* **Update Credentials**: Click the **API Settings** gear icon in the top right of your app dashboard to switch engines, select different local models, or update API keys.
* **Reset Onboarding**: If you want to switch from Cloud to Offline (or vice-versa) and start setup from scratch, click the **Reset App Configuration & Onboarding** button at the bottom of the API Settings panel.

> [!TIP]
> **LLM Provider Recommendation (Local vs. Cloud)**:
> - **Local Models (Ollama)**: While AutoShorts supports fully offline moments analysis via local Ollama models (like LLaMA 3.2 3B or Qwen 2.5 3B/7B), **local models are generally not recommended for viral moment detection**. Smaller 3B/7B models lack the context reasoning and mathematical capabilities needed to evaluate long transcripts and calculate accurate segment timestamps (often outputting fragments that are too short).
> - **DeepSeek (Highly Recommended)**: We strongly suggest using **DeepSeek** for moment detection. It offers top-tier reasoning capabilities (matching GPT-4/Claude 3.5 Sonnet) at a **fraction of a cent per run** (under $0.001 per transcript). You can get an API key instantly at [platform.deepseek.com](https://platform.deepseek.com).
> - **Claude (Premium Option)**: Claude 3.5 Sonnet provides the absolute best hooks copywriting and emotional resonance, but is slightly more expensive than DeepSeek (typically $0.01 – $0.05 per run).

---

## 🎬 Clip Mix Studio — many clips, one short

If you already have a pile of short clips (a day of filming, a trip, a shoot) and just want one finished vertical short out of them, pick **Mix clips into a short** in the sidebar.

1. **Your clips** — choose the folder holding them. Sub-folders are scanned up to 8 levels deep and files are ordered the way a human would (`clip 2` before `clip 10`). Untick anything you don't want, and use the arrows to re-order.
2. **Music** — choose a folder of your own tracks or royalty-free audio. The selected track is faded in over 1.2s and out over 1.5s, and can be layered on top of the sound already in your clips.
3. **Look & length** — pick *Fill the screen* (crop to 9:16) or *Fit whole frame on a blurred background*, choose hard cuts or crossfades, and cap the seconds per clip and the total length. Presets for 30s / 60s / 90s reels are one click away.
4. **Name & render** — name it and hit **Build my short**. Progress streams live as each clip is normalised, joined, mixed and normalised.

Everything is rendered locally with `ffmpeg` and written to:

```
<Documents>\AutoShorts\Clip Mixes\<name>\<name>.mp4
```

The output is always **1080x1920, 30 fps, H.264 + AAC 48 kHz stereo** — the geometry and loudness both Instagram Reels and YouTube Shorts want. Live renders are also listed under **Recent mixes** in the studio.

> [!IMPORTANT]
> **Music must be yours or licensed for reuse.** AutoShorts deliberately does **not** download "trending" audio from Instagram, TikTok or YouTube — ripping those tracks is copyright infringement and would get your post taken down or your account actioned. Supply your own files (including tracks you've licensed) in a folder and AutoShorts handles the rest: fades, volume balancing against your clips' own sound, and the -14 LUFS loudness normalisation.

---

## Developer Guide

> **Contributing or picking this up cold?** Read [`AGENTS.md`](AGENTS.md) first — it is the
> project memory: architecture map, feature history, verification recipes, and the
> non-obvious traps (ffmpeg filter-graph rules, the Windows build PATH/vcvars requirement,
> why the UI can't be driven from a browser tab). AI agents in VS Code also load
> [`.github/copilot-instructions.md`](.github/copilot-instructions.md) automatically.

### 1. Setup Environment Configuration
Copy `.env.example` to `.env` in the root folder:
```bash
cp .env.example .env
```
Fill in your API Keys:
```env
DEEPGRAM_API_KEY=your-deepgram-api-key
DEEPSEEK_API_KEY=your-deepseek-api-key
ANTHROPIC_API_KEY=your-anthropic-api-key

# Choose your default AI analysis provider ("deepseek" or "claude")
LLM_PROVIDER=deepseek
```

### 2. Run in Development Mode
To start the live-reloaded frontend and backend development shell:
```bash
npm install
npm run tauri:dev
```

### 3. Build the Application
To build and package the native macOS app bundle (`.app` and `.dmg` installer):
```bash
npm run tauri:build
```
The output installers will be built under `src-tauri/target/release/bundle/`.

<a id="support"></a>

## ❤️ Support AutoShorts

If AutoShorts helps you create content faster, consider supporting its development.

Your support helps fund new features, bug fixes, and ongoing improvements.

👉 https://buy.polar.sh/polar_cl_ZjNgejG1JnPQqVXyMmEmq7vpdwJUFBqx4qahw4BqBCP
