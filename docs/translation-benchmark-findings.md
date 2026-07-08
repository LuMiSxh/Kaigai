# Translation benchmark findings

Generated from the local JP VTuber corpus on 2026-07-08. Updated after the
ASR-to-MT experiments on the same date.

The benchmark corpus contains 15 clips, 1,610 seconds of audio, 7 long clips,
and 7 Tsunomaki Watame clips. Decode mode is simulated streaming unless noted:
6,000 ms windows with 600 ms overlap.

## Current recommendation

For the current one-step Whisper translation pipeline:

- Default/balanced: `medium` with Core ML.
- Fastest acceptable: `small` with Core ML.
- Accuracy-first: `large-v3` with Core ML.
- Do not recommend `tiny` or `base` for users. They are fast, but output quality
  is not good enough for VTuber streams and they repeat subtitle-like artifacts
  much more often.
- Keep `large-v3-turbo` as a transcription-only reference. It is not a drop-in
  translation model, but it is a useful benchmark for a future ASR-to-MT
  pipeline.
- Do not switch the app default to ASR-to-MT yet. The measured OPUS-MT path is
  fast enough but qualitatively worse than direct Whisper translation, and the
  measured Qwen path is too slow for live default use.

## Speed results

Lower realtime factor is faster. All rows are on the same 15-clip corpus with
the current filter pipeline.

| Model            | Backend | Task       | Avg RTF | Watame avg RTF | Long avg RTF |
| ---------------- | ------- | ---------- | ------: | -------------: | -----------: |
| `tiny`           | Core ML | translate  |   0.015 |          0.012 |        0.014 |
| `tiny`           | Metal   | translate  |   0.017 |          0.015 |        0.015 |
| `base`           | Core ML | translate  |   0.025 |          0.020 |        0.020 |
| `base`           | Metal   | translate  |   0.029 |          0.024 |        0.025 |
| `small`          | Core ML | translate  |   0.045 |          0.040 |        0.040 |
| `small`          | Metal   | translate  |   0.063 |          0.060 |        0.057 |
| `medium`         | Core ML | translate  |   0.117 |          0.107 |        0.111 |
| `medium`         | Metal   | translate  |   0.161 |          0.150 |        0.153 |
| `large-v3`       | Core ML | translate  |   0.244 |          0.223 |        0.240 |
| `large-v3`       | Metal   | translate  |   0.302 |          0.291 |        0.295 |
| `large-v3-turbo` | Core ML | transcribe |   0.110 |          0.103 |        0.105 |
| `large-v3-turbo` | Metal   | transcribe |   0.215 |          0.206 |        0.208 |

Core ML should remain the default on macOS. It was faster for every measured
OpenAI model. Tiny and Base are fast, but their translation quality is not good
enough for the product default.

## ASR-to-MT comparison

The measured ASR-to-MT implementation used:

- ASR: `large-v3-turbo` transcription with Core ML, windowed the same way as
  the direct Whisper baseline.
- MT candidate 1: `Helsinki-NLP/opus-mt-ja-en` through local Transformers on
  MPS.
- MT candidate 2: `Qwen/Qwen2.5-0.5B-Instruct` through local Transformers on
  MPS, sampled on one Watame clip because latency was already disqualifying.

| Pipeline                                    | Clips | Avg RTF | Watame avg RTF | MT avg / clip | Decision                                   |
| ------------------------------------------- | ----: | ------: | -------------: | ------------: | ------------------------------------------ |
| `large-v3-turbo` ASR + OPUS-MT, cleaned ASR |    15 |   0.123 |          0.114 |         0.96s | Not default: fast enough, but worse output |
| `large-v3-turbo` ASR + Qwen 0.5B            |     1 |   0.584 |          0.584 |        21.56s | Not default: too slow and too creative     |

OPUS-MT translated quickly, especially after ASR repetition cleanup, but it
amplified ASR mistakes and produced rigid or wrong English on Watame and gaming
clips. Example failure modes include repeated source fragments becoming English
gibberish, names/items being mistranslated, and unrelated phrases such as
"hospital" appearing from noisy ASR. Qwen produced more casual text, but it added
style/emoji and was far slower than even `large-v3` direct translation on the
45-second Watame sample.

Current decision: keep direct Whisper translation as default and use `medium`
with Core ML for new users. Product default should be Stable caption mode:
translated rolling partials are suppressed unless the user explicitly chooses
Live mode. ASR-to-MT is not faster than `medium` after the translation step, and
it is worse qualitatively in the measured OPUS/Qwen candidates. Keep ASR-to-MT
as an experimental benchmark path, not as the RC default.

## Hallucination filtering

The failure mode reported during Watame superchat reading is real: short pauses
and non-speech windows make Whisper emit stock subtitle/YouTube phrases such as
"thank you for watching", "please subscribe", "see you next time", and
translator credits.

Implemented mitigations:

- apply `no_speech_probability` thresholding before output reaches the
  stabilizer;
- suppress non-speech tokens in whisper.cpp decode params;
- remove embedded subtitle artifacts from otherwise useful segments;
- reject dominant repetition loops;
- verify ambiguous short translations such as "bye" and "thank you" by running
  a source-language decode for that window before keeping them.
- collapse long source/decoder repetition runs such as repeated Japanese
  syllables, repeated Japanese phrase units, and repeated identical words.
- suppress translated rolling partials in Stable caption mode so final
  utterance captions are favored over unstable low-latency drafts.
- reject very low-speech translation windows and weak short low-speech
  translations.

On Watame clips with `small`, `medium`, and `large-v3`, strong subtitle-artifact
phrase hits dropped from 7 before the post-filter change to 0 after it. Remaining
suspect phrases are mostly ambiguous short phrases such as "I'm sorry", which
can be real in Japanese livestream speech and should not be blanket-deleted
without stronger evidence.

## Multi-speaker streams

The current pipeline can process multi-speaker clips at similar speed, but it
does not know speaker identity. That means a future multi-speaker feature should
not be treated as only a translation setting. It likely needs a separate
experimental toggle for diarization or speaker separation, plus UI copy that
sets expectations clearly.

For 1.0, keep multi-speaker behavior as best-effort translation. Do not promise
speaker labels yet.

## Architecture track after 1.0

The two-step ASR-to-MT direction is worth testing, but it should be isolated as
an experimental engine first:

1. Japanese ASR: produce Japanese text quickly and reliably.
2. Machine translation or small local LLM: translate text with stream context.
3. Compare against current one-step Whisper translation on the same corpus.

This is not a small refactor. It changes model storage, runtime backends,
settings, latency accounting, error reporting, and possibly licensing.

Candidate ASR models to evaluate:

- SenseVoice-Small: official docs describe a non-autoregressive framework,
  Japanese support, acoustic event tags, and very low latency.
- ReazonSpeech k2/NeMo: official Reazon docs describe Japanese-focused ASR
  trained on a 35,000-hour corpus, with k2-v2 distributed in ONNX format.
- `large-v3-turbo`: already available in our stack and fast enough to be a
  local ASR reference, but it cannot translate.

Candidate MT models to evaluate:

- OPUS-MT Japanese-to-English: measured baseline for speed; not good enough as
  the default translator.
- NLLB distilled 600M: broader multilingual baseline, probably heavier than
  OPUS-MT.
- A compact local LLM such as Qwen 2.5/3-class models: useful for context and
  casual stream English, but the measured 0.5B Transformers path is too slow and
  too stylistically loose for live default use.

Rejected/low-priority for now:

- Kotoba-Whisper for Kaigai’s default path, because it is transcription-only.
- Full E2E speech translation replacements before RC unless they can run
  natively, offline, and faster than the current Core ML Whisper path.

## Sources for future-model claims

- SenseVoice: https://github.com/FunAudioLLM/SenseVoice
- ReazonSpeech project: https://research.reazon.jp/projects/ReazonSpeech/
- ReazonSpeech v2.1/k2-v2: https://research.reazon.jp/blog/2024-08-01-ReazonSpeech.html
- NVIDIA Canary docs: https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/models.html
- NVIDIA Riva Canary model card with Japanese listed: https://catalog.ngc.nvidia.com/orgs/nvidia/teams/riva/models/canary-riva-1b
- Meta SeamlessM4T: https://github.com/facebookresearch/seamless_communication
- OPUS-MT ja-en: https://huggingface.co/Helsinki-NLP/opus-mt-ja-en
- NLLB distilled 600M: https://huggingface.co/facebook/nllb-200-distilled-600M
- Qwen 2.5 overview: https://qwenlm.github.io/blog/qwen2.5/
- Whisper non-speech hallucination study: https://arxiv.org/abs/2501.11378
