<div align="center">

<img src="static/app-icon.png" width="112" alt="Kaigai app icon">

# Kaigai

**Live English subtitles for Japanese streams, generated on your own device**

Paste a YouTube link into Kaigai and use its floating subtitle bar as an always-on-top overlay.

[![Release](https://img.shields.io/github/v/release/LuMiSxh/Kaigai?include_prereleases&label=release)](https://github.com/LuMiSxh/Kaigai/releases)
[![CI](https://github.com/LuMiSxh/Kaigai/actions/workflows/ci.yml/badge.svg)](https://github.com/LuMiSxh/Kaigai/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

[Overview](#overview) • [Features](#features) • [Installation](#installation) • [Quick Start](#quick-start) • [Development](#development)

</div>

---

## Overview

Kaigai captures livestream audio, detects speech, transcribes it with Whisper, and displays readable captions without opening another video player. Speech recognition, translation, voice detection, and caption filtering run locally; the network is used only for the stream and required downloads.

```text
stream URL → yt-dlp → ffmpeg → Silero VAD → Whisper → quality filter → overlay
```

> [!WARNING]
> Kaigai is currently developed and tested only on macOS with Apple Silicon. Windows and Linux packages are built automatically but have not received end-to-end testing.

## Features

- **Local processing:** Stream audio is not uploaded to a transcription or translation API.
- **Floating overlay:** Move and resize the always-on-top subtitle bar or enable click-through mode.
- **Stable and Live captions:** Choose calmer finalized lines or lower-latency rolling drafts.
- **Adaptive speech detection:** Natural pause detection and quality filters reduce repetition loops and common false captions.
- **Managed tools:** Pinned ffmpeg and QuickJS builds are bundled; Kaigai can install and update a verified yt-dlp binary.
- **Restricted streams:** Optional browser-profile or `cookies.txt` access supports member-only and age-restricted content.
- **Configurable output:** Adjust language, speech sensitivity, timing, typography, colors, opacity, and line cutting.

## Installation

Download the package for your platform from [GitHub Releases](https://github.com/LuMiSxh/Kaigai/releases):

| Platform | Package | Status |
| --- | --- | --- |
| macOS 12+, Apple Silicon | `.dmg` | Supported and tested |
| Windows, x86-64 | `.msi` | Untested |
| Linux, x86-64 | `.AppImage` or `.deb` | Untested |

The first launch downloads a Whisper model and prepares yt-dlp. Model downloads range from roughly 74 MB to 2.9 GB. Once installed, transcription and translation work locally while the original stream still comes from its hosting site.

## Quick Start

1. Open Kaigai and choose a speech model. **Medium** with Neural Engine acceleration is the recommended balance on Apple Silicon.
2. Let Kaigai manage yt-dlp unless you already maintain a system installation.
3. Paste a YouTube URL into the floating bar and press **Enter**.
4. Move and resize the overlay over the video.
5. Use the menu-bar or tray icon for settings and session controls. Press **Esc** to stop the session and quit.

Restricted streams can be configured under **Settings → Access** before starting a session.

## Choosing a Model

| Model | Download | Recommended use | English translation |
| --- | ---: | --- | :---: |
| Tiny | 74 MB | Quick tests | Yes |
| Base | 141 MB | Light casual use | Yes |
| Small | 465 MB | Fastest usable VTuber benchmark result | Yes |
| **Medium** | **1.4 GB** | **Everyday balance** | Yes |
| Large v3 | 2.9 GB | Accuracy first | Yes |
| Large v3 Turbo | 1.5 GB | Fast source-language transcription | No |

Apple Silicon can additionally use a Core ML encoder for Neural Engine acceleration. See the [benchmark findings](docs/benchmarks/findings.md) for measurements and trade-offs.

## Known Limitations

- Whisper's built-in translation target is English.
- Large v3 Turbo supports transcription but not English translation.
- Speaker identification is not available.
- Names, slang, overlapping voices, and speech under music remain best effort.
- Streaming sites change frequently, so yt-dlp updates may be required.

## Development

Kaigai uses Rust/Tauri 2 for the desktop application and audio pipeline, with a Svelte 5 interface. Development requires Node.js 24, pnpm 11.11, Rust 1.85+, and the platform-specific Tauri dependencies.

```sh
git clone https://github.com/LuMiSxh/Kaigai.git
cd Kaigai
pnpm install --frozen-lockfile
pnpm tauri dev
```

Run the project checks with:

```sh
pnpm check
pnpm lint
pnpm test
pnpm test:rust
```

## Documentation

- [Documentation index](docs/README.md)
- [Development guide](docs/development.md)
- [Benchmark workflow and findings](docs/benchmarks/README.md)
- [Release checklist](docs/maintainers/release-checklist.md)
- [Changelog](CHANGELOG.md)

## License

Kaigai is available under either the [MIT License](LICENSE-MIT) or the [Apache License 2.0](LICENSE-APACHE).
