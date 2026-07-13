# Accuracy mode: a dual-pass design

> [!IMPORTANT]
> This is a design proposal. Kaigai does not implement dual-pass captions or a
> user-facing Accuracy mode yet.

The tempting version of Accuracy mode is “run Medium and Large v3 at the same
time, then show the better answer.” In practice, that can make both models
slower, produce late corrections and cause distracting text replacement.

The design below is more conservative. Stable captions remain the foundation.
A second model sees final speech windows only, runs when capacity is available
and may replace a caption only when its result passes a strict quality gate.

Whisper is not a native streaming model. The most useful published approach is
chunked decoding with an emission policy such as local agreement and adaptive
latency, not simply keeping two decoders busy. Kaigai's current `Stable` mode
already moves in that direction by preferring final utterances over rolling
translation drafts.

## Research behind the decision

- Whisper-Streaming adapts Whisper-like models for live transcription and
  translation through local agreement with self-adaptive latency, reporting
  robust practical behavior and about 3.3s latency on long-form speech.
  <https://arxiv.org/abs/2307.14743>
- The whisper.cpp stream example is explicitly described as a naive real-time
  example that repeatedly samples and decodes audio. This is useful as a
  baseline, but not enough as a product architecture.
  <https://github.com/ggerganov/whisper.cpp/blob/master/examples/stream/README.md>
- faster-whisper/CTranslate2 can be materially faster than the reference
  Whisper implementation and may be useful for a future backend, but that is a
  backend swap, not automatically a better caption policy.
  <https://github.com/SYSTRAN/faster-whisper>
- faster-whisper batched inference has open quality-risk reports, so batched
  throughput optimizations should not be assumed safe for live caption quality.
  <https://github.com/SYSTRAN/faster-whisper/issues/1179>

## Decision

Do not make dual-pass the default before measuring it.

Use this order:

1. Stable mode as product default.
2. Local-agreement-style finalization improvements.
3. Optional dual-pass Accuracy mode.
4. Only then consider a faster backend such as CTranslate2 or dedicated ASR.

## What the user would see

User-facing quality modes:

| Mode     | Draft behavior                   | Final behavior                             | Intended user  |
| -------- | -------------------------------- | ------------------------------------------ | -------------- |
| Fast     | optional rolling drafts          | `small` final                              | lowest latency |
| Balanced | no translation drafts by default | `medium` final                             | default        |
| Accuracy | optional subtle draft            | `medium` fast final, `large-v3` correction | quality-first  |

Accuracy mode must not append a competing second line. A correction replaces
or confirms the same caption, using a subtle transition.

## Proposed runtime

```text
RollingWindow
  ↓
Fast inference queue
  ↓
CaptionQualityGate
  ↓
Stable emission policy
  ↓
Subtitle(id, revision=0, source=fast)
  ↓ final windows only
Accuracy correction queue
  ↓
Large/accuracy inference
  ↓
CorrectionQualityGate
  ↓
SubtitleReplacement(id, revision=1, source=accuracy)
```

## Backend shape

```rust
enum CaptionQualityMode {
    Fast,
    Balanced,
    Accuracy,
}

struct CaptionEngineSet {
    fast: WhisperEngine,
    accuracy: Option<WhisperEngine>,
}

struct CaptionCandidate {
    id: u64,
    start_ms: u64,
    end_ms: u64,
    text: String,
    source: CaptionSource,
    revision: u32,
}
```

## Caption identity and revisions

Current subtitle events are append-like. A correction needs a stable caption
ID and a higher revision:

```rust
struct SubtitleEvent {
    id: u64,
    start_ms: u64,
    end_ms: u64,
    text: String,
    revision: u32,
    source: CaptionSource,
    inference_ms: u64,
}
```

The overlay keeps the active caption by `id`. If a higher revision arrives
for that ID, the text is replaced in place.

## Scheduling without hurting live captions

Do not run fast and accuracy inference concurrently by default. On Apple
Silicon, two Whisper contexts can compete for Metal/Core ML resources and
increase tail latency.

Recommended scheduler:

- fast queue remains real-time priority;
- accuracy queue consumes only final windows;
- accuracy queue is bounded to one pending correction;
- if a newer final caption arrives before correction finishes, cancel or drop the
  stale correction;
- if queue delay exceeds a threshold, skip accuracy correction for that window.

Initial thresholds:

- `accuracy_max_queue_delay_ms = 2_000`;
- `accuracy_max_total_delay_ms = 5_000`;
- `accuracy_min_speech_ms = 700`.

## When a correction is allowed

Large v3 is not automatically right. Accept a correction only when:

- candidate is non-empty;
- no hallucination/repetition/artifact gate trips;
- source window had enough speech;
- correction is not a generic outro or subscription phrase;
- correction length is plausible relative to fast text;
- correction is not semantically empty compared with fast text.

Conservative first version:

```text
accept if:
  fast is empty and accuracy passes all gates
  OR
  accuracy passes all gates
  AND normalized accuracy != normalized fast
  AND length_ratio is between 0.5 and 2.2
```

When in doubt, keep the first caption. A missed correction is less harmful
than replacing readable text with a late hallucination.

## Cost and failure modes

Dual-pass may require two loaded Whisper contexts:

- `medium` GGML + Core ML encoder;
- `large-v3` GGML + Core ML encoder.

This can increase:

- memory pressure;
- model load time;
- Core ML/Metal contention;
- power usage;
- perceived UI delay when corrections arrive late.

For that reason, Accuracy mode should begin behind a developer or advanced
setting and earn its place through measurements before becoming user-facing.

## How the design earns a release

Compare on the committed VTuber corpus:

- Balanced: `medium` direct translate + Stable.
- Accuracy baseline: `large-v3` direct translate + Stable.
- Dual-pass: `medium` fast final + `large-v3` final correction.

Evaluate separately:

- all clips;
- Watame clips;
- sparse/superchat pause clips;
- long clips;
- multi-speaker clips;
- music/karaoke transition clips.

Required before shipping Accuracy mode:

- no strong subtitle/outro artifacts;
- correction tail latency usually under 5s;
- visibly better Watame/gaming output than `medium`;
- no worse than `large-v3` direct on obvious samples;
- no distracting correction flicker in the overlay.

## Implementation order

1. Add caption IDs/revisions to backend feed and overlay.
2. Add `CaptionQualityMode` separate from `captionMode`.
3. Add hidden/dev setting for Accuracy mode.
4. Add `CaptionEngineSet` that can optionally load an accuracy model.
5. Add bounded final-window correction queue.
6. Add correction acceptance gate.
7. Add benchmark mode for dual-pass reports.
8. Only expose user-facing Accuracy after benchmark evidence.

[Back to the documentation index](../README.md) ·
[Benchmark workflow](../benchmarks/README.md) ·
[Current findings](../benchmarks/findings.md)
