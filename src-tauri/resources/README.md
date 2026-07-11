# Runtime resources

`build.rs` downloads hash-pinned ffmpeg, QuickJS and Silero VAD artifacts into
this directory for packaging. Generated binaries and pin files are ignored by
Git — see `bin/README.md` and `models/README.md`.

yt-dlp is resolved from `PATH` or managed at runtime in the app data directory.
