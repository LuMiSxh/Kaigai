# Developing Kaigai

Kaigai has a Svelte frontend and a Rust backend joined by Tauri. Most changes
touch one side only, but the settings and command types cross that boundary
through generated TypeScript bindings.

> [!WARNING]
> Day-to-day development and end-to-end testing currently happen on Apple
> Silicon macOS. Linux CI compiles and tests the code, but the Linux desktop
> package and the Windows build have not been tested as complete applications.

## What you need

- Node.js 24
- pnpm 11.11
- Rust 1.85 or newer (edition 2024)
- Git
- the [Tauri 2 system prerequisites](https://v2.tauri.app/start/prerequisites/)

For desktop-only work on macOS, the Xcode Command Line Tools are enough:

```sh
xcode-select --install
```

The Ubuntu packages used by CI are:

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```

On Windows, Tauri development needs Microsoft's C++ Build Tools and WebView2.
Remember that a successful build is not yet proof that Kaigai's overlay,
sidecars and live pipeline behave correctly there.

## Start the app

```sh
git clone https://github.com/LuMiSxh/Kaigai.git
cd Kaigai
pnpm install --frozen-lockfile
pnpm tauri dev
```

The first Rust build downloads hash-pinned ffmpeg, QuickJS and Silero VAD
artifacts for the current target. The setup tour later downloads a Whisper
model and either installs managed yt-dlp or points at a system copy.

To make a release bundle locally:

```sh
pnpm tauri build
```

## Where things live

| Path                      | What belongs there                                                          |
| ------------------------- | --------------------------------------------------------------------------- |
| `src/routes/`             | The overlay, onboarding, settings and developer windows                     |
| `src/lib/`                | Shared Svelte components, UI helpers and setting options                    |
| `src/types/bindings.ts`   | Generated Rust-to-TypeScript command and event types                        |
| `src-tauri/src/commands/` | The API exposed to the frontend                                             |
| `src-tauri/src/pipeline/` | Media decoding, VAD, Whisper inference, filtering and caption stabilization |
| `src-tauri/src/models/`   | Model catalog, downloads and Core ML support                                |
| `src-tauri/src/tools/`    | ffmpeg, QuickJS and yt-dlp discovery or management                          |
| `src-tauri/resources/`    | Build-time staged sidecars and the bundled VAD model                        |
| `benchmarks/`             | Corpus metadata plus ignored audio and result files                         |
| `scripts/`                | Corpus preparation and experimental translation runners                     |
| `docs/`                   | Contributor, benchmark, architecture and release documentation              |

The live path starts in `commands/session.rs`. From there the pipeline asks
yt-dlp for a playable stream, ffmpeg converts it to mono PCM, Silero decides
which windows contain speech, Whisper decodes them and the quality/stability
layers decide what reaches the overlay.

## Generated and downloaded files

Several useful files are intentionally not committed:

- `src-tauri/resources/bin/*` — ffmpeg and QuickJS staged by `build.rs`;
- `src-tauri/resources/models/*.bin` — the staged Silero VAD model;
- `benchmarks/corpus/audio/` — locally prepared benchmark clips;
- `benchmarks/corpus/cache/` — temporary benchmark downloads;
- `benchmarks/corpus/generated/` — the generated corpus manifest;
- `benchmarks/results/` — local benchmark reports;
- `target/`, `build/` and `.svelte-kit/` — build output.

`src/types/bindings.ts` is different: it is generated, but committed. If a
Rust command, event or shared type changes, regenerate it with:

```sh
pnpm bindings
```

Review the resulting diff before committing it. CI fails when the checked-in
bindings no longer match Rust.

## Checks

For a normal change, run:

```sh
pnpm check
pnpm lint
pnpm test
pnpm test:rust
```

The complete CI-equivalent set is:

```sh
pnpm format:check
pnpm check
pnpm lint
pnpm test
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm test:rust
pnpm bindings
git diff --exit-code -- src/types/bindings.ts
```

Run the benchmark suite only when a change could affect inference, windowing,
voice detection, filtering or model recommendations. The
[benchmark guide](benchmarks/README.md) explains the slower workflow.

## Common cross-boundary changes

### Adding or changing a setting

1. Update `AppSettings`, its default and validation in
   `src-tauri/src/settings.rs`.
2. Update the relevant Svelte settings panel and, when needed,
   `src/lib/settings-options.ts`.
3. Run `pnpm bindings`.
4. Test both a fresh setup and loading settings written by an older build.

### Changing a frontend command or event

Keep the Rust type as the source of truth, regenerate the bindings and use the
generated API from `src/types/bindings.ts`. Do not hand-edit that file.

### Adding a model

The model catalog is in `src-tauri/src/models/definitions.rs`. A model entry
needs an immutable size and SHA-256 checksum. Translation support must be
declared honestly: distilled Whisper models can transcribe well while ignoring
the English translation task.

### Updating a bundled tool

`src-tauri/build.rs` owns build-time ffmpeg, QuickJS and Silero pins. Managed
yt-dlp lives in `src-tauri/src/tools/ytdlp.rs` and is verified against the
checksums from its release. Keep target triples explicit so an unsupported
platform fails loudly instead of receiving the wrong binary.

## Benchmarks and releases

- [Run the translation benchmark](benchmarks/README.md)
- [Read the latest findings](benchmarks/findings.md)
- [Use the release checklist](maintainers/release-checklist.md)

[Back to the documentation index](README.md)
