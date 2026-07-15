# Benchmark findings

Last updated 15 July 2026 on an Apple Silicon Mac.

## Current recommendation

| Use            | Model    | Backend |
| -------------- | -------- | ------- |
| Default        | Medium   | Core ML |
| Faster         | Small    | Core ML |
| Accuracy first | Large v3 | Core ML |
| ASR reference  | Turbo v3 | Core ML |

Tiny and Base are fast but produce too many subtitle-like artifacts. Turbo v3
cannot translate to English by itself.

On the 15-clip, 1,610-second VTuber set, Core ML beat Metal for every stock
model:

| Model          | Core ML RTF | Metal RTF |
| -------------- | ----------: | --------: |
| Tiny           |       0.015 |     0.017 |
| Base           |       0.025 |     0.029 |
| Small          |       0.045 |     0.063 |
| Medium         |       0.117 |     0.161 |
| Large v3       |       0.244 |     0.302 |
| Turbo v3 (ASR) |       0.110 |     0.215 |

## Pipeline changes

Stable translation used to decode rolling drafts and discard them. Skipping
those calls reduced one 45-second Watame clip from 21 Whisper calls to 9 and
from 24.35s to 9.30s of inference. The final text was unchanged. Live mode and
same-language transcription still use rolling hypotheses.

The reference baseline contains 24 FLEURS sentences in three variants:

| Audio | chrF2++ | Avg RTF |
| ----- | ------: | ------: |
| Clean |  36.139 |   0.211 |
| 15 dB |  36.287 |   0.224 |
| 5 dB  |  35.433 |   0.198 |

No final clip was empty and no strong artifact or repetition was found. This is
a regression set, not a general translation leaderboard.

## Experiments we are not shipping

| Experiment                | Result                                                     |
| ------------------------- | ---------------------------------------------------------- |
| Kotoba Bilingual Q5       | Real-time, but added meta text and inaccurate rewrites.    |
| Turbo ASR + OPUS-MT       | Fast enough, but amplified ASR errors.                     |
| Turbo ASR + NLLB 600M     | Fast enough, worse output, non-commercial checkpoint.      |
| Turbo ASR + Qwen 0.5B     | Too slow and too creative.                                 |
| 12-second Whisper windows | p95 rose from 1.53s to 5.85s; more loops and outro text.   |
| Medium + Large dual pass  | No reliable way to know when the correction is more right. |

The proposed correction gate accepted 278 of 308 Kotoba changes and 47 of 61
NLLB changes, including bad ones. Length, repetition and artifact checks are
safety filters; they cannot judge translation accuracy. Accuracy mode stays
deferred until a candidate wins on references and real stream clips.

## Next model experiment

Try a decoder-only LoRA on Whisper Medium. Keep the six-second maximum,
independent decoding and current VAD pipeline. Compare it with stock Medium on:

- clean and noisy FLEURS scores;
- empty, repeated and artifact outputs;
- Watame, gaming, music and multi-speaker clips;
- median and p95 window latency.

If it wins, publish full GGML, quantized GGML and any Core ML bundle separately,
with hashes and data attribution. Keep it optional until it has seen wider use.

[Run the benchmarks](README.md) · [Documentation index](../README.md)
