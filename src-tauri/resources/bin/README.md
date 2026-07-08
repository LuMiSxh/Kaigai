# Sidecar binaries

`build.rs` downloads the hash-pinned ffmpeg binary for the current target
into this directory; it's gitignored and never committed.

This file exists so `resources/bin/*` (the bundle resource glob in
`tauri.conf.json`) always matches at least one file, including on CI's Linux
runners where no ffmpeg pin exists and nothing else gets staged here.
