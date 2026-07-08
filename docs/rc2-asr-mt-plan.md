# RC2 ASR-to-MT plan

Status: experimental, not default.

Direct Whisper translation remains the default for RC2 unless a future ASR-to-MT
candidate beats it on both quality and live latency on the committed VTuber
corpus.

## Decision from current measurements

Measured on 2026-07-08:

- `medium` direct Whisper translate + Core ML: avg RTF 0.117, Watame avg RTF
  0.107.
- `large-v3` direct Whisper translate + Core ML: avg RTF 0.244, Watame avg RTF
  0.223.
- `large-v3-turbo` ASR + OPUS-MT with cleaned ASR text: avg RTF 0.123,
  Watame avg RTF 0.114.
- `large-v3-turbo` ASR + Qwen 0.5B sample: RTF 0.584 on one 45s Watame clip.

The OPUS-MT path is fast enough but not better overall because output quality is
visibly worse on Watame and gaming clips, even after ASR repetition cleanup.
Qwen 0.5B is not latency-viable in the tested Transformers/MPS setup and adds
unwanted style.

## Default promotion criteria

An ASR-to-MT engine can become default only if it satisfies all of these:

- avg RTF <= `medium` direct translate on the same benchmark corpus;
- Watame avg RTF <= `medium` direct translate;
- no strong outro/subtitle artifacts on Watame clips;
- visibly better translation than `medium` on at least Watame superchat,
  Watame gaming, and one multi-speaker collab slice;
- no invented streamer intent, emoji, credits, subscription calls, or
  explanations;
- works offline after models are installed;
- model licenses and downloaded sizes are acceptable for distribution copy;
- app UI can explain the engine clearly without exposing benchmark-only tooling.

## RC2 implementation shape

Keep the app runtime on direct Whisper for now, but keep the benchmarkable
ASR-to-MT lane in-tree:

1. Maintain the corpus and result JSON workflow.
2. Keep `large-v3-turbo` as the local ASR reference.
3. Keep OPUS-MT and Qwen scripts as reproducible benchmark tools.
4. Add future candidates behind benchmark scripts before app settings.
5. Only add a user-facing engine selector after a candidate beats the default.

This prevents a worse engine from becoming product surface area while still
making ASR-to-MT measurable.

## Next candidates

### NLLB distilled 600M

Priority: medium.

Reason: likely better than OPUS-MT on semantic translation, still more
deterministic than a general LLM. Risk is size and latency.

Benchmark target:

- same `large-v3-turbo` ASR report;
- full 15-clip corpus;
- compare against `medium` and `large-v3`.

### CTranslate2 / ct2rs

Priority: high if NMT quality is acceptable.

Reason: this is the most plausible path from Python benchmark to native app
runtime. CTranslate2 supports MarianMT/NLLB-class models and has Rust bindings
through `ct2rs`.

Risk: extra native dependency and model conversion pipeline.

### SenseVoice or ReazonSpeech ASR

Priority: high for the ASR side, but only after MT quality is solved.

Reason: Gemini's strongest architectural point is replacing Whisper ASR with a
Japanese-optimized/non-autoregressive ASR. Current measurements show
`large-v3-turbo` ASR is fast enough that MT quality is the bigger blocker.

Risk: new runtime backend, model packaging, licensing, and platform support.

### Local LLM translation

Priority: low for default, medium for optional quality experiments.

Reason: context handling is attractive, but Qwen 0.5B via Transformers/MPS was
too slow and too creative. A quantized llama.cpp/MLX setup may be better, but it
must prove latency and deterministic style before entering the app.

## Product UI when ready

Do not expose model-stack internals first. If a future candidate wins, expose:

- Engine: `Live translation` / `Experimental ASR + translator`;
- Quality mode: `Fast`, `Balanced`, `Accuracy`;
- optional advanced details for selected ASR and MT models.

For RC2, user-facing model recommendation should stay:

- Balanced: Medium + Neural Engine.
- Fast: Small + Neural Engine.
- Accuracy: Large v3 + Neural Engine.
