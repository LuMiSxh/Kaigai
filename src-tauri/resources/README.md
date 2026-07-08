# Runtime resources

`build.rs` downloads hash-pinned ffmpeg and Silero VAD artifacts into this
directory for packaging. Generated binaries and pin files are ignored by Git.

yt-dlp is resolved from `PATH` or managed at runtime in the app data directory.
