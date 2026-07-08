# Translation benchmark suite

Kaigai benchmarks live-caption behavior against short regression clips and
longer stress clips from Japanese VTuber streams. Only public video metadata
and timestamps are committed — audio is generated locally and gitignored.

## Corpus goals

The corpus intentionally covers cases that stress different parts of the
pipeline:

- short and long superchat-reading utterances with 1-3 seconds of silence,
  where Whisper tends to hallucinate closers such as "bye", "thank you", or
  "see you next time";
- Watame-heavy coverage, because Tsunomaki Watame is the primary real-world
  target channel for Kaigai;
- long-running segments with sparse, balanced, dense, and music-heavy speech
  density, because the app is expected to run continuously, not only on short
  isolated samples;
- single-speaker gaming with fast reactions and background audio;
- multi-speaker gaming/collab streams with overlap risk;
- music/karaoke transitions, where non-speech filtering matters;
- mixed Japanese/English collabs, which will be relevant if speaker/language
  features become toggleable later.

Each clip has `lengthProfile`, `speechDensity`, `speakerProfile`,
`speakerCount`, and `overlapRisk` metadata so results can be grouped by
single-speaker versus multi-speaker behavior, short versus long behavior, and
sparse versus dense speech instead of averaged away.

## Prepare audio

```bash
pnpm bench:prepare
```

This creates:

- `benchmarks/corpus/audio/*.wav`
- `benchmarks/corpus/cache/*`
- `benchmarks/corpus/generated/manifest.json`

The generated WAV files are mono 16 kHz PCM. The prepare script downloads only
the requested timestamp sections via `yt-dlp --download-sections`; it does not
cache full livestream archives. The current corpus is about 27 minutes total,
with roughly half of that focused on Watame streams.

Useful overrides:

```bash
KAIGAI_BENCH_REBUILD=1 pnpm bench:prepare
KAIGAI_BENCH_CORPUS=/path/to/corpus.json pnpm bench:prepare
YT_DLP=/path/to/yt-dlp FFMPEG=/path/to/ffmpeg pnpm bench:prepare
```

## Run the model matrix

```bash
pnpm bench:models
```

The runner reads `benchmarks/corpus/generated/manifest.json` and writes:

```text
benchmarks/results/model-matrix.json
```

It measures:

- model load time;
- inference time;
- realtime factor;
- output text;
- empty-output count after filtering;
- backend (`coreml` or isolated `metal`);
- task (`translate` or `transcribe`);
- single-speaker/multi-speaker metadata.
- short/long and speech-density metadata.

By default the runner uses `KAIGAI_BENCH_DECODE=streaming`: clips are decoded
through simulated live windows (`6000ms` window, `600ms` overlap). This is the
recommendation path. `KAIGAI_BENCH_DECODE=whole` is available only as a
diagnostic because full-clip Whisper calls are not how the app runs and are much
more prone to repetition loops.

On macOS, Core ML is measured using the installed model directory. The no-Core
ML path is measured through temporary symlink directories under
`$TMPDIR/kaigai-bench-no-coreml`, which intentionally do not contain
`.mlmodelc` bundles. The installed models are not modified.

The expected installed model matrix is:

- `tiny`
- `base`
- `small`
- `medium`
- `large-v3`
- `large-v3-turbo` for transcription-only speed comparison

`kotoba-whisper-v2.0` is intentionally excluded because it does not support the
translation task that Kaigai primarily optimizes for.

Useful overrides:

```bash
KAIGAI_MODEL_DIR="$HOME/Library/Application Support/com.lumisxh.kaigai/models" pnpm bench:models
KAIGAI_BENCH_OUTPUT=benchmarks/results/custom.json pnpm bench:models
KAIGAI_BENCH_MODELS=small,medium KAIGAI_BENCH_TASKS=translate KAIGAI_BENCH_BACKENDS=coreml pnpm bench:models
KAIGAI_BENCH_WINDOW_MS=6000 KAIGAI_BENCH_OVERLAP_MS=600 pnpm bench:models
```

## Criterion microbenchmarks

Criterion is available for focused inference measurements:

```bash
pnpm bench:criterion
```

Optional selectors:

```bash
KAIGAI_BENCH_MODEL=small KAIGAI_BENCH_BACKEND=coreml KAIGAI_BENCH_TASK=translate pnpm bench:criterion
KAIGAI_BENCH_MODEL=small KAIGAI_BENCH_BACKEND=metal KAIGAI_BENCH_TASK=translate pnpm bench:criterion
```

Use Criterion when comparing a narrow pipeline/code change. Use
`pnpm bench:models` for recommendation work because it captures the full model
matrix and output text.

## Decoupled ASR-to-MT experiments

The ASR-to-MT runners consume a windowed transcription report from
`pnpm bench:models`. Generate one first:

```bash
KAIGAI_BENCH_MODELS=large-v3-turbo \
KAIGAI_BENCH_TASKS=transcribe \
KAIGAI_BENCH_BACKENDS=coreml \
KAIGAI_BENCH_OUTPUT=benchmarks/results/asr-turbo-coreml-transcribe-windows.json \
pnpm bench:models
```

Then run a local OPUS-MT translation pass:

```bash
pnpm bench:asr-mt:opus -- \
  --input benchmarks/results/asr-turbo-coreml-transcribe-windows.json \
  --output benchmarks/results/asr-mt-turbo-opus-transformers.json \
  --model Helsinki-NLP/opus-mt-ja-en \
  --batch-size 8 \
  --device auto
```

For local LLM translation experiments:

```bash
pnpm bench:asr-mt:qwen -- \
  --input benchmarks/results/asr-turbo-coreml-transcribe-windows.json \
  --output benchmarks/results/asr-mt-turbo-qwen05-oneclip.json \
  --model Qwen/Qwen2.5-0.5B-Instruct \
  --device auto \
  --clip-filter watame-superchat-short-pauses-001
```

These scripts intentionally use `uv` inline Python dependencies. They are
benchmark tools, not production app dependencies.

## Reading results

For a release recommendation, evaluate at least these slices separately:

- all clips, translate task;
- `speaker=Tsunomaki Watame`, translate task;
- `lengthProfile=long`, translate task;
- `speechDensity=sparse`, translate task;
- `speechDensity=dense`, translate task;
- `speakerProfile=single`, translate task;
- `speakerProfile=multiple`, translate task;
- `category=superchat_short_pauses`, translate task;
- `category=music_background` and `music_talk_transition`, translate task;
- transcribe-only reference for `large-v3-turbo`.

The final recommendation should name:

- fastest viable model;
- best balanced model;
- best accuracy-first model;
- whether Core ML should be default for the model;
- whether multi-speaker streams need a future diarization/speaker-separation
  toggle instead of only filtering changes.

The current measured recommendation and architecture notes are tracked in
[translation-benchmark-findings.md](translation-benchmark-findings.md).
