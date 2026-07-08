# Release quality gates

Kaigai optimizes for readable, trustworthy captions over the lowest possible
latency — that's why `Stable` caption mode is the default.

## Caption modes

### Stable

Default for translated livestreams.

- renders final utterance captions;
- suppresses rolling translation drafts;
- keeps the last good caption visible during short pauses;
- rejects very low-speech translation outputs;
- rejects duplicate/empty partials and dominant repetition loops.

Use this for Watame-style superchat reading, zatsudan, and streams with frequent
1-3 second pauses.

### Live

Optional low-latency mode.

- allows rolling partials after the quality gate;
- lower perceived latency;
- more caption churn and more risk from unstable Whisper hypotheses.

Use this only when latency is more important than caption stability.

## RC quality checks

Before promoting a release candidate, run the VTuber corpus and check these
slices:

- all clips;
- `speaker=Tsunomaki Watame`;
- `category=superchat_short_pauses`;
- `lengthProfile=long`;
- `speakerProfile=multiple`;
- `category=music_background`;
- `category=music_talk_transition`.

Required acceptance:

- zero strong subtitle/outro artifacts in Watame outputs:
    - "thank you for watching";
    - "please subscribe";
    - "see you next time";
    - "translated by".
- no UI partial updates in Stable translation mode;
- very low-speech translation windows do not render captions;
- `medium` + Core ML remains comfortably real-time on the corpus;
- `small` remains available as the Fast option;
- `large-v3` remains available as the Accuracy option;
- ASR-to-MT stays experimental until it beats `medium` + Core ML on quality and
  latency.
- Dual-pass Accuracy stays experimental until it proves better final captions
  without distracting replacement flicker or unacceptable correction latency.

## Benchmark commands

```bash
pnpm bench:prepare
KAIGAI_BENCH_MODELS=small,medium,large-v3 \
KAIGAI_BENCH_TASKS=translate \
KAIGAI_BENCH_BACKENDS=coreml \
KAIGAI_BENCH_OUTPUT=benchmarks/results/rc-quality-coreml-translate.json \
pnpm bench:models
```

For the ASR-to-MT lane:

```bash
KAIGAI_BENCH_MODELS=large-v3-turbo \
KAIGAI_BENCH_TASKS=transcribe \
KAIGAI_BENCH_BACKENDS=coreml \
KAIGAI_BENCH_OUTPUT=benchmarks/results/rc-quality-asr.json \
pnpm bench:models

pnpm bench:asr-mt:opus -- \
  --input benchmarks/results/rc-quality-asr.json \
  --output benchmarks/results/rc-quality-asr-mt-opus.json
```
