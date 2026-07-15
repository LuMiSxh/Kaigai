# Releasing Kaigai

This is the ship-or-stop checklist for a Kaigai release. The automated suite
can catch broken code and stale bindings; it cannot tell whether a caption is
pleasant to read for an hour or whether a packaged overlay behaves correctly.
Both kinds of checks matter.

> [!WARNING]
> Windows and Linux are not end-to-end tested today. A green Linux CI job means
> the code compiles and its tests pass; it does not make the packaged Linux app
> supported. Keep both platforms marked **untested** in release notes until
> someone completes and records the smoke test below on real systems.

## 1. Prepare the release

- [ ] Choose the version and whether it is a prerelease.
- [ ] Set the same version in `package.json`,
      `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.
- [ ] Turn the matching `CHANGELOG.md` section from `Unreleased` into a dated
      release entry.
- [ ] Read the root README as a new user. Package names, model advice, platform
      status and known limitations must still be true.
- [ ] Check that model and sidecar download URLs, sizes and SHA-256 pins are
      immutable and match the intended files.
- [ ] Make sure no architecture proposal is described as an implemented
      feature.

## 2. Run the normal checks

```sh
pnpm install --frozen-lockfile
pnpm format:check
pnpm check
pnpm lint
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features --locked -- -D warnings
pnpm test:rust
pnpm bindings
git diff --exit-code -- src/types/bindings.ts
```

- [ ] All commands pass from a clean checkout.
- [ ] Regenerating bindings leaves no diff.
- [ ] The release commit contains no benchmark audio, local model files,
      generated reports, credentials or cookie files.

## 3. Check caption quality

Kaigai favors readable, trustworthy captions over the smallest possible
latency. That is why `Stable` is the default.

### Stable mode

- [ ] Shows final utterance captions rather than rolling translation drafts.
- [ ] Keeps the last useful line visible through short pauses.
- [ ] Rejects empty duplicates, dominant repetition loops and very-low-speech
      translation output.
- [ ] Does not produce obvious “thank you for watching”, “please subscribe”,
      “see you next time” or translator-credit artifacts on Watame clips.

### Live mode

- [ ] Shows rolling partials with lower perceived latency.
- [ ] Replaces the current draft instead of stacking duplicate lines.
- [ ] Makes the stability trade-off clear in the UI.

Run the release model slice:

```sh
pnpm bench:prepare
KAIGAI_BENCH_MODELS=small,medium,large-v3 \
KAIGAI_BENCH_TASKS=translate \
KAIGAI_BENCH_BACKENDS=coreml \
KAIGAI_BENCH_OUTPUT=benchmarks/results/release-coreml-translate.json \
pnpm bench:models
```

Review all clips, then check these slices separately:

- [ ] `speaker=Tsunomaki Watame`
- [ ] `category=superchat_short_pauses`
- [ ] `lengthProfile=long`
- [ ] `speakerProfile=multiple`
- [ ] `category=music_background`
- [ ] `category=music_talk_transition`

The release is blocked if:

- Medium + Core ML is no longer comfortably real-time;
- Small is no longer a credible fast option;
- Large v3 is no longer the best direct accuracy option;
- a filter change removes genuine speech to hide a bad metric;
- strong outro or subscription hallucinations return;
- Stable mode shows translated rolling drafts;
- a model recommendation changed without a report and an explanation in the
  [benchmark findings](../benchmarks/findings.md).

ASR-to-MT and the proposed dual-pass Accuracy mode stay experimental until
they satisfy their documented quality and latency criteria.

## 4. Smoke-test the packaged app

Builds run differently from `pnpm tauri dev`. Test the package a user will
actually download.

### macOS / Apple Silicon

- [ ] Install the DMG on a macOS 12-or-newer machine.
- [ ] Complete onboarding from a fresh app data directory.
- [ ] Download Medium and its Core ML data; verify checksum errors fail safely.
- [ ] Install managed yt-dlp and resolve a public YouTube livestream.
- [ ] Start, stop and restart a subtitle session.
- [ ] Confirm the overlay moves, resizes, stays on top and supports
      click-through.
- [ ] Change caption appearance, timing and mode; restart and confirm settings
      persist.
- [ ] Exercise browser or `cookies.txt` access on a restricted test stream
      when credentials are available.
- [ ] Confirm the tray/menu-bar controls and Esc quit path stop child processes.
- [ ] Leave a stream running long enough to see pause handling and reconnect
      behavior.

### Windows / x86-64

- [ ] Install the MSI on a clean x86-64 Windows machine.
- [ ] Repeat the onboarding, model, yt-dlp, stream, overlay and shutdown checks
      above.
- [ ] Confirm ffmpeg and QuickJS sidecars launch without PATH changes.
- [ ] Confirm the frameless transparent always-on-top window and click-through
      behavior work with WebView2.
- [ ] Record the Windows version and hardware used.

Until every Windows item is checked, call the Windows package **available but
untested**, not supported.

### Linux / x86-64

- [ ] Install the DEB on a clean Debian/Ubuntu-family machine.
- [ ] Launch the AppImage on a second clean environment when possible.
- [ ] Repeat the onboarding, model, yt-dlp, stream, overlay and shutdown checks
      above.
- [ ] Confirm tray support, transparency, always-on-top and click-through under
      the tested desktop environment and display server.
- [ ] Confirm bundled sidecars have executable permissions.
- [ ] Record the distribution, desktop environment and Wayland/X11 session.

Until every Linux item is checked, call the Linux packages **available but
untested**, not supported.

## 5. Publish

The `publish` GitHub Actions workflow is started manually.

- [ ] Push the release commit.
- [ ] Run `.github/workflows/publish.yml` with a `v<version>` tag that
      exactly matches all three version files.
- [ ] Select the prerelease flag correctly.
- [ ] Wait for macOS, Windows and Linux jobs; one successful target must not
      hide a failed target.
- [ ] Review the draft release title and the notes extracted from
      `CHANGELOG.md`.
- [ ] Confirm the expected DMG, MSI, AppImage and DEB assets are present.
- [ ] Put the Windows/Linux testing warning near the top of the release notes.
- [ ] Download and open the macOS asset from the draft itself before publishing.
- [ ] Publish the draft only after the artifact and quality checks are complete.

## 6. After publishing

- [ ] Verify the README release link and badge resolve to the new version.
- [ ] Install once from the public release page rather than a local artifact.
- [ ] Watch the first issue reports for stream resolution, model download and
      platform-specific packaging failures.
- [ ] Add confirmed regressions to the changelog and benchmark corpus where
      they can be reproduced.

[Back to the documentation index](../README.md) ·
[Development guide](../development.md) ·
[Benchmark workflow](../benchmarks/README.md)
