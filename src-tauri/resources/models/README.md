# Runtime models

`build.rs` downloads the hash-pinned Silero VAD model for the current target
into this directory; the `.bin`/`.pin` files are gitignored and never
committed.

This file exists so `resources/models/*` (the bundle resource glob in
`tauri.conf.json`) always matches at least one file, including on CI's Linux
runners where no model is staged and nothing else gets put here.
