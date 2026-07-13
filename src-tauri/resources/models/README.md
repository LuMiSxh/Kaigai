# Runtime models

`build.rs` downloads the hash-pinned Silero VAD model for the current target
into this directory; the `.bin`/`.pin` files are gitignored and never
committed.

This file keeps the `resources/models/*` bundle glob valid before
`build.rs` has staged the model. It is packaged alongside the generated
artifact and is not used by the runtime.
