# Changelog

All notable changes to Kaigai are documented in this file.

## [Unreleased]

### Added

- Full-pipeline benchmark mode covering Silero VAD, adaptive finalization,
  stabilization and caption quality decisions.
- Automated FLEURS Japanese-to-English reference preparation, clean/noisy
  chrF2++ scoring and a local decoder-LoRA training probe.

### Changed

- Stable translation now skips rolling Whisper calls that can never be shown.
  On the measured 45-second Watame clip this reduced inference calls from 21 to
  9 and inference time from 24.35s to 9.30s with identical final caption text.
- Benchmark-only Rust code and WAV dependencies are excluded from production
  builds and shared report/path logic is no longer duplicated by entry points.

## [1.0.0] - 2026-07-13

### Added

- Local Whisper transcription and translation with Silero voice detection.
- Adaptive live subtitles, model management and Apple Core ML support.
- Managed yt-dlp, bundled ffmpeg and authenticated stream access.
- Bundled QuickJS-NG runtime so yt-dlp can solve YouTube's "n" signature
  challenge without needing Deno/Node on the user's PATH.
- Tray controls, onboarding, settings and a debug-only developer console.
- VTuber-focused translation benchmark suite with Watame-heavy long-form clips,
  Core ML vs. Metal model matrix tooling and Criterion hooks.
- Experimental ASR-to-MT benchmark tooling for `large-v3-turbo` ASR with OPUS-MT
  and Qwen translation candidates.
- Stable caption mode for translated streams, with Live mode available when
  lower latency is preferred over caption stability.
- Release quality-gate documentation and an architecture plan for future
  dual-pass Accuracy mode.
- Linux release builds (AppImage/deb), alongside macOS and Windows.

### Changed

- Updated the UI to Anasthasia 0.2.0 and its namespaced design tokens.
- Changed the default model recommendation to Medium with Core ML after the
  VTuber corpus showed it is the best balanced default.
- Tightened Whisper hallucination filtering for short pauses, subtitle outro
  artifacts, translator credits and repeated decoder loops.
- Suppressed unstable rolling translation drafts in Stable caption mode so final
  utterance captions are favored over low-latency churn.
- Improved model setup copy to describe Fast, Balanced and Accuracy tradeoffs.
- The caption bar now keeps up to two lines on screen (newest at the bottom)
  instead of flash-replacing a caption before it can be read.
- Raised the default end-of-utterance silence cut from 250ms to 600ms so
  breath pauses no longer split sentences across captions.

### Fixed

- Managed yt-dlp installs on Linux now fetch the Linux binary instead of the
  macOS one.
- The inference backend shown in the developer console now reports CPU
  correctly on Windows/Linux instead of always claiming Metal.
- Windows/Linux ffmpeg pins now point at an immutable BtbN build instead of
  their rolling `latest` tag, whose contents change in place and broke the
  checksum pin without warning.

### Removed

- Removed Kotoba Whisper from the selectable model catalog because it does not
  support English translation.
