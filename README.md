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

More on the benchmark suite (VTuber corpus, Core ML/no-Core ML matrix,
multi-speaker slicing) and the current model recommendation, including the
ASR-to-MT track, is in
[docs/translation-benchmarks.md](docs/translation-benchmarks.md) and
[docs/translation-benchmark-findings.md](docs/translation-benchmark-findings.md).
Release quality criteria live in
[docs/release-quality-gates.md](docs/release-quality-gates.md), and the
dual-pass Accuracy mode plan is in
[docs/dual-pass-accuracy-architecture.md](docs/dual-pass-accuracy-architecture.md).

## License

Kaigai is licensed under either the MIT License or Apache License 2.0, at
your option.
