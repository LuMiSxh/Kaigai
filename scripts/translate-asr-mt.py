# /// script
# dependencies = [
#   "torch>=2.4",
#   "transformers>=4.45,<5",
#   "sentencepiece>=0.2",
#   "sacremoses>=0.1",
# ]
# ///
"""Translate a windowed ASR benchmark report with a local MT model.

This is intentionally a benchmark/experiment tool, not the production runtime.
It lets Kaigai compare the current one-step Whisper translate path against a
decoupled ASR -> MT path on the exact same corpus and windowing.
"""

from __future__ import annotations

import argparse
import json
import re
import statistics
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import torch
from transformers import AutoModelForSeq2SeqLM, AutoTokenizer


DEFAULT_MODEL = "Helsinki-NLP/opus-mt-ja-en"


@dataclass
class Translation:
    text: str
    milliseconds: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model", default=DEFAULT_MODEL)
    parser.add_argument("--batch-size", default=8, type=int)
    parser.add_argument(
        "--device",
        choices=["auto", "cpu", "mps"],
        default="auto",
        help="MPS is faster on some Macs, but CPU is usually more deterministic.",
    )
    return parser.parse_args()


def choose_device(requested: str) -> torch.device:
    if requested == "cpu":
        return torch.device("cpu")
    if requested == "mps":
        return torch.device("mps")
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def load_translator(model_name: str, device: torch.device) -> tuple[Any, Any, int]:
    started = time.perf_counter()
    tokenizer = AutoTokenizer.from_pretrained(model_name)
    model = AutoModelForSeq2SeqLM.from_pretrained(model_name)
    model.to(device)
    model.eval()
    load_ms = int((time.perf_counter() - started) * 1000)
    return tokenizer, model, load_ms


def clean_asr_text(text: str) -> str:
    text = normalize_spaces(text)
    text = collapse_repeated_latin_words(text)
    text = collapse_repeated_cjk_bigrams(text)
    text = collapse_repeated_char_runs(text)
    return normalize_spaces(text)


def normalize_spaces(text: str) -> str:
    return " ".join(text.split())


def collapse_repeated_latin_words(text: str) -> str:
    # Whisper often loops "LALALA" or repeated English fragments over karaoke.
    return re.sub(r"\b([A-Za-z]{2,})(?:\s+\1\b){4,}", r"\1", text, flags=re.IGNORECASE)


def collapse_repeated_cjk_bigrams(text: str) -> str:
    # Keep two repetitions, then drop the rest. This catches examples like
    # スカスカスカ... without deleting normal Japanese emphasis.
    pattern = re.compile(r"((?:[\u3040-\u30ff\u3400-\u9fff]{2,6})){4,}")

    def replace(match: re.Match[str]) -> str:
        value = match.group(0)
        for size in range(2, 7):
            if len(value) % size != 0:
                continue
            unit = value[:size]
            if unit * (len(value) // size) == value:
                return unit * 2
        return value

    previous = None
    while previous != text:
        previous = text
        text = pattern.sub(replace, text)
    return text


def collapse_repeated_char_runs(text: str) -> str:
    # Avoid sending huge karaoke/non-speech character loops into MT.
    return re.sub(r"([A-Za-zぁ-んァ-ヶ一-龯])\1{8,}", lambda m: m.group(1) * 4, text)


def translate_texts(
    tokenizer: Any,
    model: Any,
    device: torch.device,
    texts: list[str],
    batch_size: int,
) -> dict[str, Translation]:
    translations: dict[str, Translation] = {}
    unique_texts = [text for text in dict.fromkeys(texts) if text]
    for start in range(0, len(unique_texts), batch_size):
        batch = unique_texts[start : start + batch_size]
        encoded = tokenizer(
            batch,
            return_tensors="pt",
            padding=True,
            truncation=True,
            max_length=512,
        )
        encoded = {key: value.to(device) for key, value in encoded.items()}
        started = time.perf_counter()
        with torch.inference_mode():
            generated = model.generate(
                **encoded,
                max_new_tokens=128,
                num_beams=1,
                do_sample=False,
            )
        elapsed_ms = (time.perf_counter() - started) * 1000
        decoded = tokenizer.batch_decode(generated, skip_special_tokens=True)
        per_item_ms = elapsed_ms / max(1, len(batch))
        for source, target in zip(batch, decoded, strict=True):
            translations[source] = Translation(
                text=normalize_spaces(target),
                milliseconds=per_item_ms,
            )
    return translations


def join_stable_text(texts: list[str]) -> str:
    output: list[str] = []
    for text in texts:
        text = normalize_spaces(text)
        if not text:
            continue
        if output and output[-1] == text:
            continue
        output.append(text)
    return " ".join(output)


def summarize(runs: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, str], list[dict[str, Any]]] = {}
    for run in runs:
        grouped.setdefault((run["model"], run["backend"], run["task"]), []).append(run)
    rows = []
    for (model, backend, task), group in sorted(grouped.items()):
        rtfs = sorted(run["realtimeFactor"] for run in group)
        watame = [
            run["realtimeFactor"]
            for run in group
            if run["speaker"] == "Tsunomaki Watame"
        ]
        long = [
            run["realtimeFactor"]
            for run in group
            if run["lengthProfile"] == "long"
        ]
        multi = [
            run["realtimeFactor"]
            for run in group
            if run["speakerProfile"] == "multiple"
        ]
        rows.append(
            {
                "model": model,
                "backend": backend,
                "task": task,
                "clipCount": len(group),
                "averageRealtimeFactor": sum(rtfs) / len(rtfs),
                "medianRealtimeFactor": statistics.median(rtfs),
                "averageInferenceMs": sum(run["inferenceMs"] for run in group)
                / len(group),
                "averageAsrInferenceMs": sum(run["asrInferenceMs"] for run in group)
                / len(group),
                "averageMtInferenceMs": sum(run["mtInferenceMs"] for run in group)
                / len(group),
                "totalEmptyOutputs": sum(1 for run in group if not run["text"]),
                "watameAverageRealtimeFactor": (sum(watame) / len(watame))
                if watame
                else None,
                "longAverageRealtimeFactor": (sum(long) / len(long)) if long else None,
                "multiSpeakerAverageRealtimeFactor": (sum(multi) / len(multi))
                if multi
                else None,
            }
        )
    return rows


def main() -> None:
    args = parse_args()
    report = json.loads(args.input.read_text())
    device = choose_device(args.device)
    tokenizer, model, mt_load_ms = load_translator(args.model, device)

    window_sources: list[str] = []
    for run in report["runs"]:
        for window in run.get("windows", []):
            cleaned = clean_asr_text(window.get("text", ""))
            window["cleanedAsrText"] = cleaned
            if cleaned:
                window_sources.append(cleaned)

    translated = translate_texts(
        tokenizer,
        model,
        device,
        window_sources,
        args.batch_size,
    )

    runs: list[dict[str, Any]] = []
    for run in report["runs"]:
        translated_windows = []
        translated_texts = []
        mt_ms = 0.0
        for window in run.get("windows", []):
            source = window.get("cleanedAsrText", "")
            translation = translated.get(source, Translation("", 0.0))
            mt_ms += translation.milliseconds
            translated_texts.append(translation.text)
            translated_windows.append(
                {
                    "index": window["index"],
                    "startMs": window["startMs"],
                    "endMs": window["endMs"],
                    "asrInferenceMs": window["inferenceMs"],
                    "mtInferenceMs": round(translation.milliseconds, 3),
                    "asrText": source,
                    "text": translation.text,
                }
            )

        duration_ms = run["durationMs"]
        asr_ms = run["inferenceMs"]
        total_ms = int(round(asr_ms + mt_ms))
        runs.append(
            {
                **{
                    key: run[key]
                    for key in [
                        "clipId",
                        "category",
                        "lengthProfile",
                        "speechDensity",
                        "speakerProfile",
                        "speakerCount",
                        "overlapRisk",
                        "speaker",
                        "durationMs",
                        "windowMs",
                        "overlapMs",
                        "windowCount",
                    ]
                },
                "model": f"{run['model']}+{args.model}",
                "backend": f"{run['backend']}+transformers/{device.type}",
                "task": "asr-mt",
                "decodeMode": run["decodeMode"],
                "loadMs": run["loadMs"] + mt_load_ms,
                "inferenceMs": total_ms,
                "asrInferenceMs": asr_ms,
                "mtInferenceMs": round(mt_ms, 3),
                "realtimeFactor": total_ms / max(1, duration_ms),
                "segments": sum(1 for text in translated_texts if text),
                "text": join_stable_text(translated_texts),
                "asrText": run["text"],
                "windows": translated_windows,
            }
        )

    output = {
        "generatedAt": str(int(time.time())),
        "sourceReport": str(args.input),
        "asrModel": report["runs"][0]["model"] if report.get("runs") else None,
        "mtModel": args.model,
        "mtDevice": device.type,
        "mtLoadMs": mt_load_ms,
        "batchSize": args.batch_size,
        "corpus": report["corpus"],
        "decodeMode": report["decodeMode"],
        "windowMs": report["windowMs"],
        "overlapMs": report["overlapMs"],
        "clipCount": len(runs),
        "runs": runs,
        "summary": summarize(runs),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n")
    print(args.output)
    print(json.dumps(output["summary"], ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
