# Sidecar binaries

`build.rs` downloads the hash-pinned ffmpeg and QuickJS binaries for the
current target into this directory; both are gitignored and never committed.

This file exists so `resources/bin/*` (the bundle resource glob in
`tauri.conf.json`) always matches at least one file, even before `build.rs`
has staged anything.
