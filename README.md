# Kaigai

Kaigai generates local English subtitles for Japanese livestreams. It resolves
the stream with yt-dlp and runs Whisper on-device; audio is not uploaded.

## Development

```bash
pnpm install
pnpm tauri dev
```

Before submitting a change:

```bash
pnpm check
pnpm lint
pnpm test
pnpm test:rust
```

Translation/model benchmark workflow:

```bash
pnpm bench:prepare
pnpm bench:models
```

See [docs/translation-benchmarks.md](docs/translation-benchmarks.md) for the
VTuber corpus, Core ML/no-Core ML matrix, and multi-speaker result slicing.
The current recommendation is in
[docs/translation-benchmark-findings.md](docs/translation-benchmark-findings.md).
The ASR-to-MT RC2 track is in [docs/rc2-asr-mt-plan.md](docs/rc2-asr-mt-plan.md).
Release quality criteria are in
[docs/release-quality-gates.md](docs/release-quality-gates.md).
Dual-pass Accuracy planning is in
[docs/dual-pass-accuracy-architecture.md](docs/dual-pass-accuracy-architecture.md).

## License

Kaigai is licensed under either the MIT License or Apache License 2.0, at
your option.
