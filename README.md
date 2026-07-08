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

## License

Kaigai is licensed under either the MIT License or Apache License 2.0, at
your option.
