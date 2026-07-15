# Benchmarking

Kaigai has two local benchmark sets:

- VTuber clips for realistic latency, noise and hallucination checks;
- FLEURS Japanese audio with English references for chrF2++ scoring.

Generated audio and reports are ignored by Git. The VTuber manifest only stores
public URLs and timestamps. FLEURS is pinned to a dataset revision and licensed
CC-BY-4.0.

## Prepare data

```sh
# VTuber clips. Requires yt-dlp and ffmpeg.
pnpm bench:prepare

# 24 FLEURS sentences, clean plus 15 dB and 5 dB noise variants.
pnpm bench:prepare:reference
```

Useful overrides:

```sh
KAIGAI_BENCH_REBUILD=1 pnpm bench:prepare
KAIGAI_BENCH_CORPUS=/path/to/corpus.json pnpm bench:prepare

python3 scripts/prepare-fleurs-corpus.py \
  --split dev --limit 100 --snr-db 20,10,5 \
  --output benchmarks/reference/generated/fleurs-dev.json
```

## Run models

```sh
pnpm bench:models
```

The runner finds models in Kaigai's data directory. Limit a run with environment
variables:

```sh
KAIGAI_BENCH_MODELS=small,medium \
KAIGAI_BENCH_TASKS=translate \
KAIGAI_BENCH_BACKENDS=coreml \
KAIGAI_BENCH_OUTPUT=benchmarks/results/medium.json \
pnpm bench:models
```

Decode modes:

| Mode        | Use                                                        |
| ----------- | ---------------------------------------------------------- |
| `streaming` | Fixed 6s windows; stresses decoder hallucinations.          |
| `pipeline`  | Runs VAD, adaptive windows, stabilization and quality gates. |
| `whole`     | Diagnostic only; not how the app runs.                       |

Use `pipeline` before making a product recommendation:

```sh
KAIGAI_BENCH_DECODE=pipeline \
KAIGAI_BENCH_MODELS=medium \
KAIGAI_BENCH_TASKS=translate \
KAIGAI_BENCH_BACKENDS=coreml \
pnpm bench:models
```

The pipeline report includes finalization reasons, speech time, decoded text,
quality decisions and emitted text. Window size, overlap, VAD sensitivity and
silence timing can be overridden with the corresponding `KAIGAI_BENCH_*`
variables in `src-tauri/examples/bench_corpus.rs`.

## Custom models

```sh
KAIGAI_BENCH_MODEL_PATH=/path/to/ggml-model.bin \
KAIGAI_BENCH_MODEL_ID=my-model \
KAIGAI_BENCH_MODELS=my-model \
KAIGAI_BENCH_TASKS=translate \
KAIGAI_BENCH_BACKENDS=metal \
KAIGAI_BENCH_LANGUAGE=ja \
pnpm bench:models
```

Set `KAIGAI_BENCH_MODEL_SUPPORTS_TRANSLATE=false` for ASR-only models. Kotoba
Bilingual expects `KAIGAI_BENCH_LANGUAGE=en`.

For repeated timing of one model, use `pnpm bench:criterion`. It accepts the
same custom model path and language variables.

## Score and compare

Run Medium on the reference set:

```sh
KAIGAI_BENCH_CORPUS=benchmarks/reference/generated/fleurs-ja-en.json \
KAIGAI_BENCH_DECODE=pipeline \
KAIGAI_BENCH_MODELS=medium \
KAIGAI_BENCH_TASKS=translate \
KAIGAI_BENCH_BACKENDS=coreml \
KAIGAI_BENCH_OUTPUT=benchmarks/results/fleurs-medium.json \
pnpm bench:models

pnpm bench:score \
  --input benchmarks/results/fleurs-medium.json \
  --output benchmarks/results/fleurs-medium-scores.json
```

Compare aligned model reports with:

```sh
pnpm bench:compare \
  --baseline benchmarks/results/medium.json \
  --candidate benchmarks/results/candidate.json \
  --output benchmarks/results/comparison.json
```

chrF2++ catches semantic regressions on clean read speech. The VTuber set still
needs a quick text review for names, jokes, music and overlapping speakers.

## Fine-tuning probe

Prepare separate FLEURS splits:

```sh
python3 scripts/prepare-fleurs-corpus.py \
  --split train --limit 1000 --snr-db 20,10,5 \
  --output benchmarks/reference/generated/fleurs-train.json

python3 scripts/prepare-fleurs-corpus.py \
  --split dev --limit 100 --snr-db 15,5 \
  --output benchmarks/reference/generated/fleurs-dev.json
```

Then run the decoder-only Medium LoRA probe:

```sh
pnpm train:whisper \
  --train-manifest benchmarks/reference/generated/fleurs-train.json \
  --eval-manifest benchmarks/reference/generated/fleurs-dev.json \
  --max-steps 100 \
  --output training/whisper-medium-ja-en-lora
```

The script uses MPS, batch size one, gradient accumulation and a frozen encoder.
It will use most of a 16 GB Mac while training. Use `--merge-output` to create a
checkpoint that whisper.cpp can convert; Kaigai cannot load a LoRA adapter
directly.

A candidate only moves forward if it improves clean and noisy reference scores,
does not add empty or hallucinated captions, stays real-time, and looks better
on the VTuber clips. Ship it as an optional model first.

The ASR-to-MT and local-LLM scripts remain under `scripts/` for experiments.
They are not app features. See [current findings](findings.md) for why.
