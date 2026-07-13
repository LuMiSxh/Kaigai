# Benchmarking Kaigai

This suite answers a practical question: which local model produces useful
captions quickly enough for a real Japanese livestream?

It is not a generic Whisper leaderboard. The corpus deliberately leans toward
Kaigai's difficult day-to-day material—VTuber chat, gaming, music, short
pauses and overlapping speakers—and the runner decodes the audio in the same
small windows used by the app.

The latest interpretation of the numbers lives in
[the benchmark findings](findings.md).

## What is committed

Only public video metadata, timestamps and hand-written clip labels are kept in
Git:

```text
benchmarks/corpus/jp-vtuber-corpus.json
```

Audio is downloaded and cut locally. The WAV files, caches, generated manifest
and all result reports are ignored. That keeps the repository small and avoids
redistributing stream audio.

## What the corpus tries to catch

The clips are chosen to expose failure modes that disappear in clean,
single-sentence samples:

- superchat and chat utterances separated by one-to-three-second pauses, where
  Whisper may invent “bye”, “thank you” or “see you next time”;
- long, Watame-heavy segments because Tsunomaki Watame is Kaigai's primary
  real-world target;
- sparse, balanced, dense and music-heavy speech over longer sessions;
- fast gaming reactions over background audio;
- collabs with several voices and a risk of overlapping speech;
- karaoke or music-to-speech transitions;
- mixed Japanese and English.

Every clip carries `lengthProfile`, `speechDensity`, `speakerProfile`,
`speakerCount` and `overlapRisk` metadata. Do not look only at the overall
average: those slices are how regressions are found.

## Before running it

Set up the project using the [development guide](../development.md). Corpus
preparation also needs working `yt-dlp` and `ffmpeg` commands. They can be
on `PATH` or supplied through the environment variables shown below.

The full model matrix takes significant disk space. Install the models from
Kaigai first, or point the runner at a directory that already contains them.

## 1. Prepare the audio

```sh
pnpm bench:prepare
```

The script downloads only the requested timestamp ranges with yt-dlp and turns
them into mono 16 kHz PCM WAV files. It does not cache complete livestream
archives.

Generated files:

```text
benchmarks/corpus/audio/*.wav
benchmarks/corpus/cache/*
benchmarks/corpus/generated/manifest.json
```

Useful overrides:

```sh
KAIGAI_BENCH_REBUILD=1 pnpm bench:prepare
KAIGAI_BENCH_CORPUS=/path/to/corpus.json pnpm bench:prepare
YT_DLP=/path/to/yt-dlp FFMPEG=/path/to/ffmpeg pnpm bench:prepare
```

## 2. Run the model matrix

```sh
pnpm bench:models
```

The default report is written to:

```text
benchmarks/results/model-matrix.json
```

For every clip and model, the report records:

- model load and inference time;
- realtime factor (RTF);
- decoded text;
- output removed by the quality filters;
- backend and task;
- the corpus metadata needed for grouped comparisons.

### Streaming versus whole-clip decoding

The default, `KAIGAI_BENCH_DECODE=streaming`, simulates the live pipeline
with 6,000 ms windows and 600 ms overlap. Use this when making product or model
recommendations.

`KAIGAI_BENCH_DECODE=whole` is a diagnostic. A full-clip Whisper call is not
how Kaigai runs and is much more vulnerable to repetition loops, so its result
must not replace the streaming measurement.

### Backends on macOS

Core ML uses the installed model directory. The Metal-only run points at
temporary directories under `$TMPDIR/kaigai-bench-no-coreml` that omit the
`.mlmodelc` bundles. The runner does not modify the installed models.

The normal matrix is:

- Tiny
- Base
- Small
- Medium
- Large v3
- Large v3 Turbo as a transcription-only speed reference

Kotoba Whisper is intentionally absent because it cannot perform the English
translation task Kaigai is built around.

Useful overrides:

```sh
KAIGAI_MODEL_DIR="$HOME/Library/Application Support/com.lumisxh.kaigai/models" pnpm bench:models
KAIGAI_BENCH_OUTPUT=benchmarks/results/custom.json pnpm bench:models
KAIGAI_BENCH_MODELS=small,medium KAIGAI_BENCH_TASKS=translate KAIGAI_BENCH_BACKENDS=coreml pnpm bench:models
KAIGAI_BENCH_WINDOW_MS=6000 KAIGAI_BENCH_OVERLAP_MS=600 pnpm bench:models
```

## Focused Criterion runs

Use Criterion when comparing a narrow inference or pipeline change:

```sh
pnpm bench:criterion
```

To select one model and backend:

```sh
KAIGAI_BENCH_MODEL=small KAIGAI_BENCH_BACKEND=coreml KAIGAI_BENCH_TASK=translate pnpm bench:criterion
KAIGAI_BENCH_MODEL=small KAIGAI_BENCH_BACKEND=metal KAIGAI_BENCH_TASK=translate pnpm bench:criterion
```

Criterion is good for timing a code path. The model matrix is better for
recommendations because it also preserves output text and corpus slices.

## Experimental ASR-to-MT runs

These experiments split source transcription from English translation. They
are benchmark tools, not app features.

First produce a windowed Large v3 Turbo transcription report:

```sh
KAIGAI_BENCH_MODELS=large-v3-turbo \
KAIGAI_BENCH_TASKS=transcribe \
KAIGAI_BENCH_BACKENDS=coreml \
KAIGAI_BENCH_OUTPUT=benchmarks/results/asr-turbo-coreml-transcribe-windows.json \
pnpm bench:models
```

Then feed it to OPUS-MT:

```sh
pnpm bench:asr-mt:opus -- \
  --input benchmarks/results/asr-turbo-coreml-transcribe-windows.json \
  --output benchmarks/results/asr-mt-turbo-opus-transformers.json \
  --model Helsinki-NLP/opus-mt-ja-en \
  --batch-size 8 \
  --device auto
```

Or run the local Qwen experiment on a selected clip:

```sh
pnpm bench:asr-mt:qwen -- \
  --input benchmarks/results/asr-turbo-coreml-transcribe-windows.json \
  --output benchmarks/results/asr-mt-turbo-qwen05-oneclip.json \
  --model Qwen/Qwen2.5-0.5B-Instruct \
  --device auto \
  --clip-filter watame-superchat-short-pauses-001
```

Both scripts use `uv` inline Python dependencies so they stay outside the
production application.

## Reading a report

Always compare at least:

- all translation clips;
- Watame clips;
- long clips;
- sparse and dense speech;
- single- and multi-speaker clips;
- superchat pause clips;
- music background and music-to-talk transitions;
- Large v3 Turbo transcription as a speed reference.

A recommendation should answer five things plainly:

1. What is the fastest model that is still useful?
2. What should a new user get by default?
3. What is the accuracy-first option?
4. Is Core ML worth enabling for that model?
5. Did any slice get worse even if the average improved?

Do not promote a model on speed alone. Read its caption text, check the
hallucination categories and explain any recommendation change in
[the findings](findings.md).

[Back to the documentation index](../README.md)
