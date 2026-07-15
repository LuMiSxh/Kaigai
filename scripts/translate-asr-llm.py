# /// script
# dependencies = [
#   "torch>=2.4",
#   "transformers>=4.45,<5",
#   "sentencepiece>=0.2",
#   "accelerate>=0.34",
# ]
# ///
"""Translate a windowed ASR benchmark report with a local instruction LLM."""

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
from transformers import AutoModelForCausalLM, AutoTokenizer


DEFAULT_MODEL = "Qwen/Qwen2.5-0.5B-Instruct"


@dataclass
class WindowTranslation:
    text: str
    milliseconds: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model", default=DEFAULT_MODEL)
    parser.add_argument("--device", choices=["auto", "cpu", "mps"], default="auto")
    parser.add_argument("--max-new-tokens", default=96, type=int)
    parser.add_argument("--context-windows", default=2, type=int)
    parser.add_argument(
        "--clip-filter",
        default="",
        help="Comma-separated clip ids for quick experiments. Empty means all clips.",
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


def load_model(model_name: str, device: torch.device) -> tuple[Any, Any, int]:
    started = time.perf_counter()
    tokenizer = AutoTokenizer.from_pretrained(model_name, trust_remote_code=True)
    model = AutoModelForCausalLM.from_pretrained(
        model_name,
        torch_dtype=torch.float16 if device.type == "mps" else torch.float32,
        trust_remote_code=True,
    )
    model.to(device)
    model.eval()
    if tokenizer.pad_token_id is None:
        tokenizer.pad_token = tokenizer.eos_token
    return tokenizer, model, int((time.perf_counter() - started) * 1000)


def clean_asr_text(text: str) -> str:
    text = " ".join(text.split())
    text = re.sub(r"\b([A-Za-z]{2,})(?:\s+\1\b){4,}", r"\1", text, flags=re.I)
    text = re.sub(r"([A-Za-zぁ-んァ-ヶ一-龯])\1{8,}", lambda m: m.group(1) * 4, text)
    return text.strip()


def build_prompt(current: str, context: list[str]) -> str:
    context_text = "\n".join(f"- {item}" for item in context if item)
    return (
        "Translate Japanese livestream captions into natural, casual English.\n"
        "The speaker is a Japanese VTuber. Preserve meaning, gaming terms, names, "
        "and jokes. Do not add outros, credits, subscriptions, or explanations. "
        "If the input is only music, laughter, filler, or ASR noise, output an empty string.\n"
        + (f"Recent context:\n{context_text}\n" if context_text else "")
        + f"Japanese: {current}\nEnglish:"
    )


def strip_response(text: str) -> str:
    text = text.strip()
    for marker in ["Japanese:", "Explanation:", "Note:", "\n\n"]:
        if marker in text:
            text = text.split(marker, 1)[0].strip()
    text = re.sub(r"^English:\s*", "", text, flags=re.I).strip()
    if text in {'""', "''"}:
        return ""
    return " ".join(text.strip("\"' ").split())


def translate_window(
    tokenizer: Any,
    model: Any,
    device: torch.device,
    prompt: str,
    max_new_tokens: int,
) -> WindowTranslation:
    encoded = tokenizer(prompt, return_tensors="pt", truncation=True, max_length=1024)
    encoded = {key: value.to(device) for key, value in encoded.items()}
    started = time.perf_counter()
    with torch.inference_mode():
        generated = model.generate(
            **encoded,
            max_new_tokens=max_new_tokens,
            do_sample=False,
            temperature=None,
            top_p=None,
            pad_token_id=tokenizer.eos_token_id,
        )
    elapsed = (time.perf_counter() - started) * 1000
    new_tokens = generated[0, encoded["input_ids"].shape[1] :]
    text = tokenizer.decode(new_tokens, skip_special_tokens=True)
    return WindowTranslation(strip_response(text), elapsed)


def join_stable_text(texts: list[str]) -> str:
    output: list[str] = []
    for text in texts:
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
        rtfs = [run["realtimeFactor"] for run in group]
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
                "watameAverageRealtimeFactor": sum(watame) / len(watame)
                if watame
                else None,
                "longAverageRealtimeFactor": sum(long) / len(long) if long else None,
                "multiSpeakerAverageRealtimeFactor": sum(multi) / len(multi)
                if multi
                else None,
            }
        )
    return rows


def main() -> None:
    args = parse_args()
    if args.max_new_tokens < 1:
        raise ValueError("--max-new-tokens must be positive")
    if args.context_windows < 0:
        raise ValueError("--context-windows cannot be negative")
    clip_filter = {value.strip() for value in args.clip_filter.split(",") if value.strip()}
    source = json.loads(args.input.read_text())
    device = choose_device(args.device)
    tokenizer, model, mt_load_ms = load_model(args.model, device)

    runs = []
    for source_run in source["runs"]:
        if clip_filter and source_run["clipId"] not in clip_filter:
            continue
        context: list[str] = []
        translated_windows = []
        translated_texts = []
        mt_ms = 0.0
        for window in source_run.get("windows", []):
            asr_text = clean_asr_text(window.get("text", ""))
            if not asr_text:
                translation = WindowTranslation("", 0.0)
            else:
                prompt = build_prompt(asr_text, context[-args.context_windows :])
                translation = translate_window(
                    tokenizer,
                    model,
                    device,
                    prompt,
                    args.max_new_tokens,
                )
            mt_ms += translation.milliseconds
            if translation.text:
                context.append(translation.text)
            translated_texts.append(translation.text)
            translated_windows.append(
                {
                    "index": window["index"],
                    "startMs": window["startMs"],
                    "endMs": window["endMs"],
                    "asrInferenceMs": window["inferenceMs"],
                    "mtInferenceMs": round(translation.milliseconds, 3),
                    "asrText": asr_text,
                    "text": translation.text,
                }
            )

        total_ms = int(round(source_run["inferenceMs"] + mt_ms))
        duration_ms = source_run["durationMs"]
        runs.append(
            {
                **{
                    key: source_run[key]
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
                "model": f"{source_run['model']}+{args.model}",
                "backend": f"{source_run['backend']}+transformers/{device.type}",
                "task": "asr-mt-llm",
                "decodeMode": source_run["decodeMode"],
                "loadMs": source_run["loadMs"] + mt_load_ms,
                "inferenceMs": total_ms,
                "asrInferenceMs": source_run["inferenceMs"],
                "mtInferenceMs": round(mt_ms, 3),
                "realtimeFactor": total_ms / max(1, duration_ms),
                "segments": sum(1 for text in translated_texts if text),
                "text": join_stable_text(translated_texts),
                "asrText": source_run["text"],
                "windows": translated_windows,
            }
        )
        print(
            source_run["clipId"],
            f"mt_ms={mt_ms:.0f}",
            f"rtf={runs[-1]['realtimeFactor']:.3f}",
            flush=True,
        )

    output = {
        "generatedAt": str(int(time.time())),
        "sourceReport": str(args.input),
        "asrModel": source["runs"][0]["model"] if source.get("runs") else None,
        "mtModel": args.model,
        "mtDevice": device.type,
        "mtLoadMs": mt_load_ms,
        "maxNewTokens": args.max_new_tokens,
        "contextWindows": args.context_windows,
        "corpus": source["corpus"],
        "decodeMode": source["decodeMode"],
        "windowMs": source["windowMs"],
        "overlapMs": source["overlapMs"],
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
