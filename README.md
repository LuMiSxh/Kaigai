<div align="center">

<img src="static/app-icon.png" width="112" alt="Kaigai app icon">

# Kaigai

**Live English subtitles for Japanese streams, generated on your own device**

Paste a YouTube link into Kaigai and the small floating bar turns into a subtitle
overlay. Speech recognition and translation run locally with Whisper; your audio
is not sent to a transcription service.

[![Release](https://img.shields.io/github/v/release/LuMiSxh/Kaigai?include_prereleases&label=release)](https://github.com/LuMiSxh/Kaigai/releases)
[![CI](https://github.com/LuMiSxh/Kaigai/actions/workflows/ci.yml/badge.svg)](https://github.com/LuMiSxh/Kaigai/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)

[Overview](#overview) • [Features](#features) • [Installation](#installation) • [Quick Start](#quick-start) •
[Models](#choosing-a-model) • [Development](#development)

</div>

---

## Overview

Kaigai is made for the kind of livestream audio that neat demo clips tend to
avoid: long chats, gaming, music, quick reactions, multiple voices and awkward
pauses. It listens for speech, turns it into readable captions and keeps the
result in a lightweight always-on-top bar instead of opening another video
player.

> [!WARNING]
> Kaigai is currently developed and tested only on **macOS with Apple Silicon**.
> Windows and Linux packages are built automatically, but neither platform has
> received end-to-end testing yet. Expect platform-specific problems and please
> report anything you find.

## Features

### The speech processing stays local

Whisper, Silero voice detection and the caption quality filters all run on your
computer. Kaigai still needs a network connection to fetch the livestream and
to download models or tool updates, but it does not upload the stream audio to a
speech or translation API.

### It behaves like an overlay

The input bar becomes the subtitle display as soon as a session starts. Move it
over the video, resize it, keep it above other windows or let mouse clicks pass
through while captions are live. Fonts, colors, opacity, size and subtitle
timing are configurable.

### The defaults favor readable captions

`Stable` mode waits for a proper utterance boundary and avoids showing every
uncertain Whisper draft. `Live` mode is available when lower latency matters
more than a steady line. Adaptive voice detection cuts on natural pauses, and
the quality gate filters common repetition loops and bogus YouTube-style outro
phrases.

### The fiddly tools are handled for you

Kaigai ships with pinned builds of ffmpeg and QuickJS. It can also install and
update its own verified copy of yt-dlp, so a normal setup does not require those
tools on your `PATH`. If you already manage yt-dlp yourself, Kaigai can use
that copy instead.

### Restricted streams are supported

For member-only or age-restricted streams, Kaigai can pass a browser profile or
a `cookies.txt` file to yt-dlp. Leave this disabled for public streams.

The runtime path is deliberately simple:

```text
stream URL → yt-dlp → ffmpeg → Silero VAD → Whisper → quality filter → overlay
```

## Installation

Download the package for your platform from
[GitHub Releases](https://github.com/LuMiSxh/Kaigai/releases).

| Platform                 | Package                | Status               |
| ------------------------ | ---------------------- | -------------------- |
| macOS 12+, Apple Silicon | `.dmg`                 | Supported and tested |
| Windows, x86-64          | `.msi`                 | **Untested**         |
| Linux, x86-64            | `.AppImage` or `.deb`  | **Untested**         |

On macOS, open the DMG and drag Kaigai into `Applications`. The first launch
walks you through downloading a Whisper model and preparing yt-dlp.

> [!NOTE]
> The first setup needs an internet connection. Model downloads range from
> roughly 74 MB to 2.9 GB. Once a model is installed, transcription and
> translation happen locally; the original livestream still comes from its
> hosting site.

## Quick Start

1. Open Kaigai and choose a speech model. **Medium** with Neural Engine
   acceleration is the recommended balance on Apple Silicon.
2. Let Kaigai manage yt-dlp, unless you already keep a system installation up
   to date.
3. Paste a YouTube URL into the floating bar and press **Enter**.
4. Drag the overlay over the video and resize it from a corner.
5. Use the menu-bar or tray icon for settings and session controls. Press
   **Esc** to quit Kaigai and stop the active session.

For a restricted stream, open **Settings → Access** and select a browser
profile or cookie file before starting.

### Settings worth knowing

| Setting            | What it changes                                                       |
| ------------------ | --------------------------------------------------------------------- |
| Subtitle output    | Translate speech to English, or keep it in the source language        |
| Source language    | Japanese, Korean, Chinese, English or automatic detection             |
| Caption mode       | `Stable` for calmer final captions; `Live` for rolling drafts         |
| Line cutting       | Adaptive pauses or fixed-duration windows                             |
| Speech sensitivity | Keep quiet voices or reject more music and background noise           |
| Appearance         | Font, size, weight, colors, opacity, delay and click-through behavior |

## Choosing a model

The model is downloaded once and kept on your machine. These sizes are
approximate and do not include optional Core ML data.

| Model          |   Download | Best use                                                   | English translation |
| -------------- | ---------: | ---------------------------------------------------------- | :-----------------: |
| Tiny           |      74 MB | Quick tests; noticeably error-prone on Japanese            |         Yes         |
| Base           |     141 MB | Casual use when occasional mistakes are acceptable         |         Yes         |
| Small          |     465 MB | Fastest model that remained usable in the VTuber benchmark |         Yes         |
| **Medium**     | **1.4 GB** | **Recommended balance of speed and accuracy**              |         Yes         |
| Large v3       |     2.9 GB | Accuracy first; slower and more demanding                  |         Yes         |
| Large v3 Turbo |     1.5 GB | Fast source-language transcription                         |       **No**        |

On Apple Silicon, Core ML can download an additional encoder for Neural Engine
acceleration. The current benchmark recommends **Small** for speed,
**Medium** for everyday use and **Large v3** when accuracy matters most. The
measurements and trade-offs are documented in
[the benchmark findings](docs/benchmarks/findings.md).

## Known limitations

- Windows and Linux have not been tested end to end.
- Whisper's built-in translation target is English.
- Large v3 Turbo is transcription-only and ignores the translation task.
- Multi-speaker streams are best effort; Kaigai does not identify or label
  individual speakers.
- Live machine translation can still mishear names, slang and speech under
  music. A larger model helps, but no model is perfect.
- Streaming sites change frequently. Managed yt-dlp updates are enabled by
  default for that reason.

## Development

Kaigai uses Svelte 5 for the interface and Rust/Tauri 2 for the desktop app and
audio pipeline. You will need Node.js 24, pnpm 11.11, Rust 1.85 or newer and
the platform dependencies required by Tauri.

```sh
git clone https://github.com/LuMiSxh/Kaigai.git
cd Kaigai
pnpm install --frozen-lockfile
pnpm tauri dev
```

Run the regular checks before submitting a change:

```sh
pnpm check
pnpm lint
pnpm test
pnpm test:rust
```

The [development guide](docs/development.md) covers platform setup, repository
layout, generated files and the full CI command set.

## Documentation

- [Documentation index](docs/README.md)
- [Development guide](docs/development.md)
- [Benchmark workflow](docs/benchmarks/README.md)
- [Current benchmark findings](docs/benchmarks/findings.md)
- [Planned Accuracy mode](docs/architecture/accuracy-mode.md)
- [Release checklist](docs/maintainers/release-checklist.md)
- [Changelog](CHANGELOG.md)

## License

Kaigai is available under either the [MIT License](LICENSE-MIT) or the
[Apache License 2.0](LICENSE-APACHE), at your option.

---

<div align="center">

**An open-source project by LuMiSxh**

[GitHub](https://github.com/LuMiSxh/Kaigai) •
[Issues](https://github.com/LuMiSxh/Kaigai/issues) •
[Releases](https://github.com/LuMiSxh/Kaigai/releases)

</div>
