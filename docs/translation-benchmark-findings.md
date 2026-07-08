# Translation benchmark findings

Numbers below come from the local JP VTuber corpus (15 clips, 1,610 seconds
of audio, 7 long clips, 7 Tsunomaki Watame clips), measured 2026-07-08 and
updated the same day after the ASR-to-MT experiments. Decode mode is
simulated streaming unless noted: 6,000 ms windows with 600 ms overlap.

## Current recommendation

- Default/balanced: `medium` with Core ML.
- Fastest acceptable: `small` with Core ML.
- Accuracy-first: `large-v3` with Core ML.
- `tiny` and `base` are fast but not good enough for VTuber streams — they
  repeat subtitle-like artifacts far more often, so we don't recommend them.
- `large-v3-turbo` stays in as a transcription-only reference. It can't
  translate on its own, but it's the fastest local ASR we have, which makes
  it the natural anchor for any future ASR-to-MT pipeline.
- ASR-to-MT is not becoming the app default yet (see below) — direct Whisper
  translation with `medium` + Core ML remains what ships.

## Speed results

Lower realtime factor is faster. All rows are the same 15-clip corpus with
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

Core ML wins for every OpenAI model we measured, so it stays the macOS
default. Tiny and Base are fast enough to be tempting, but their translation
quality just isn't there yet.

## ASR-to-MT: where it stands

Direct Whisper translation stays the RC2 default. An ASR-to-MT candidate can
replace it, but only once it beats `medium` direct translate on _both_
quality and live latency on this corpus — not one or the other.

What we tried:

- ASR: `large-v3-turbo` transcription with Core ML, windowed the same way as
  the direct Whisper baseline.
- MT candidate 1: `Helsinki-NLP/opus-mt-ja-en` through local Transformers on
  MPS.
- MT candidate 2: `Qwen/Qwen2.5-0.5B-Instruct` through local Transformers on
  MPS, sampled on one Watame clip because latency already ruled it out.

| Pipeline                                    | Clips | Avg RTF | Watame avg RTF | MT avg / clip | Decision                                   |
| ------------------------------------------- | ----: | ------: | -------------: | ------------: | ------------------------------------------ |
| `large-v3-turbo` ASR + OPUS-MT, cleaned ASR |    15 |   0.123 |          0.114 |         0.96s | Not default: fast enough, but worse output |
| `large-v3-turbo` ASR + Qwen 0.5B            |     1 |   0.584 |          0.584 |        21.56s | Not default: too slow and too creative     |

OPUS-MT is fast, especially once ASR repetition is cleaned up, but it
amplifies ASR mistakes: repeated source fragments turn into English
gibberish, names and items get mistranslated, and odd phrases like
"hospital" show up out of nowhere from noisy ASR. Qwen reads more casually,
but it adds its own style and emoji, and it's far slower than even
`large-v3` direct translation on the 45-second Watame sample we tried it on.

Net: keep `medium` + Core ML as the shipping default, Stable caption mode on
by default (translated rolling partials suppressed unless the user opts into
Live mode), and treat ASR-to-MT as an experimental, benchmarkable path — not
product surface area — until a candidate actually wins on the numbers.

### What it would take to promote an ASR-to-MT engine

All of these, not just some:

- avg RTF <= `medium` direct translate on this corpus;
- Watame avg RTF <= `medium` direct translate;
- no strong outro/subtitle artifacts on Watame clips;
- visibly better translation than `medium` on at least Watame superchat,
  Watame gaming, and one multi-speaker collab slice;
- no invented streamer intent, emoji, credits, subscription calls, or
  explanations;
- works offline once models are installed;
- model licenses and download sizes are fine for distribution;
- the UI can explain the engine without exposing benchmark-only internals.

Until then, the plan is to keep the app runtime on direct Whisper and keep
the ASR-to-MT lane benchmarkable in-tree: maintain the corpus/result JSON
workflow, keep `large-v3-turbo` as the ASR reference, keep the OPUS-MT/Qwen
scripts as reproducible benchmark tools, and only add a user-facing engine
selector once a candidate actually beats the default. That keeps a worse
engine from becoming product surface area while still letting us measure
progress.

### Next candidates worth trying

**NLLB distilled 600M** (medium priority) — probably better than OPUS-MT on
semantic translation and still more deterministic than a general LLM. Main
risk is size and latency. Benchmark the same way: `large-v3-turbo` ASR
report, full 15-clip corpus, compared against `medium` and `large-v3`.

**CTranslate2 / `ct2rs`** (high priority, if NMT quality holds up) — the
most plausible path from a Python benchmark to a native Rust runtime.
Supports MarianMT/NLLB-class models with Rust bindings already available.
Costs an extra native dependency and a model conversion pipeline.

**SenseVoice or ReazonSpeech ASR** (high priority for the ASR side, but only
once MT quality is solved) — the strongest case for a Japanese-optimized,
non-autoregressive ASR to replace Whisper. Right now `large-v3-turbo` is
already fast enough that MT quality is the bigger blocker, not ASR speed.
Costs a new runtime backend, model packaging, and licensing/platform work.

**Local LLM translation** (low priority for default, medium for
experiments) — context-aware translation is appealing, but Qwen 0.5B via
Transformers/MPS was too slow and too creative. A quantized llama.cpp/MLX
setup might do better, but it has to prove latency and a deterministic style
before it's allowed anywhere near the app.

### If something wins: product UI

Don't expose model-stack internals up front. If a future candidate wins,
surface it as:

- Engine: `Live translation` / `Experimental ASR + translator`;
- Quality mode: `Fast`, `Balanced`, `Accuracy`;
- optional advanced details for the selected ASR and MT models.

Until then, the RC2 model recommendation copy stays: Balanced = Medium +
Neural Engine, Fast = Small + Neural Engine, Accuracy = Large v3 + Neural
Engine.

## Hallucination filtering

The Watame superchat-reading failure mode is real: short pauses and
non-speech windows make Whisper emit stock subtitle/YouTube phrases like
"thank you for watching", "please subscribe", "see you next time", and
translator credits.

What's in place to catch it:

- `no_speech_probability` thresholding before output reaches the stabilizer;
- non-speech token suppression in whisper.cpp decode params;
- stripping embedded subtitle artifacts out of otherwise-useful segments;
- rejecting dominant repetition loops;
- verifying ambiguous short translations such as "bye" and "thank you" by
  running a source-language decode for that window before keeping them;
- collapsing long source/decoder repetition runs — repeated Japanese
  syllables, repeated phrase units, repeated identical words;
- suppressing translated rolling partials in Stable mode so final
  utterances win over unstable low-latency drafts;
- rejecting very low-speech translation windows and weak short low-speech
  translations.

On Watame clips across `small`, `medium`, and `large-v3`, strong
subtitle-artifact hits dropped from 7 before this filter pass to 0 after.
What's left is mostly ambiguous short phrases like "I'm sorry" — those can be
genuine Japanese livestream speech, so we're not blanket-deleting them
without better evidence.

## Multi-speaker streams

The pipeline handles multi-speaker clips at about the same speed, but it has
no idea who's speaking. So a future multi-speaker feature isn't just a
translation setting — it probably needs its own experimental toggle for
diarization or speaker separation, plus UI copy that sets expectations
honestly. For 1.0, multi-speaker stays best-effort translation with no
speaker labels promised.

## Architecture track after 1.0

The two-step ASR-to-MT direction is worth pursuing, but as an isolated
experimental engine first:

1. Japanese ASR: produce Japanese text quickly and reliably.
2. Machine translation or a small local LLM: translate with stream context.
3. Compare against the current one-step Whisper translation on the same
   corpus.

This isn't a small refactor — it touches model storage, runtime backends,
settings, latency accounting, error reporting, and possibly licensing.

Candidate ASR models: SenseVoice-Small (non-autoregressive, Japanese
support, acoustic event tags, very low latency per its docs), ReazonSpeech
k2/NeMo (Japanese-focused, trained on a 35,000-hour corpus, k2-v2 ships in
ONNX), and `large-v3-turbo` (already in our stack, fast, but can't
translate).

Candidate MT models: OPUS-MT ja-en (our measured speed baseline, not good
enough as the default translator), NLLB distilled 600M (broader multilingual
baseline, probably heavier than OPUS-MT), and compact local LLMs in the Qwen
2.5/3 class (useful for context and casual stream English, but the measured
0.5B Transformers path was too slow and too loose stylistically for live
default use).

Rejected or low priority for now: Kotoba-Whisper (transcription-only, so it
doesn't fit Kaigai's translation-first path), and any full E2E speech
translation replacement before RC unless it can run natively, offline, and
faster than the current Core ML Whisper path.

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
