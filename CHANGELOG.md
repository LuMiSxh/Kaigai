# Changelog

All notable changes to Kaigai are documented in this file.

## [1.0.0-rc.1] - Unreleased

### Added

- Local Whisper transcription and translation with Silero voice detection.
- Adaptive live subtitles, model management and Apple Core ML support.
- Managed yt-dlp, bundled ffmpeg and authenticated stream access.
- Tray controls, onboarding, settings and a debug-only developer console.
- VTuber-focused translation benchmark suite with Watame-heavy long-form clips,
  Core ML vs. Metal model matrix tooling and Criterion hooks.
- Experimental ASR-to-MT benchmark tooling for `large-v3-turbo` ASR with OPUS-MT
  and Qwen translation candidates.
- Stable caption mode for translated streams, with Live mode available when
  lower latency is preferred over caption stability.
- Release quality-gate documentation and an architecture plan for future
  dual-pass Accuracy mode.

### Changed

- Updated the UI to Anasthasia 0.2.0 and its namespaced design tokens.
- Changed the default model recommendation to Medium with Core ML after the
  VTuber corpus showed it is the best balanced default.
- Tightened Whisper hallucination filtering for short pauses, subtitle outro
  artifacts, translator credits and repeated decoder loops.
- Suppressed unstable rolling translation drafts in Stable caption mode so final
  utterance captions are favored over low-latency churn.
- Improved model setup copy to describe Fast, Balanced and Accuracy tradeoffs.

### Removed

- Removed Kotoba Whisper from the selectable model catalog because it does not
  support English translation.
