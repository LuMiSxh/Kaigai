#!/usr/bin/env python3
"""Compare two aligned Kaigai benchmark reports."""

from __future__ import annotations

import argparse
import json
import math
import statistics
from pathlib import Path
from typing import Any

from benchmark_text import contains_artifact, contains_repetition, normalize


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--max-total-delay-ms", default=5_000, type=float)
    parser.add_argument("--min-length-ratio", default=0.5, type=float)
    parser.add_argument("--max-length-ratio", default=2.2, type=float)
    parser.add_argument(
        "--clip-filter",
        default="",
        help="Comma-separated clip ids to compare. Empty requires identical reports.",
    )
    return parser.parse_args()


def window_latency_ms(window: dict[str, Any]) -> float:
    if "inferenceMs" in window:
        return float(window["inferenceMs"])
    return float(window.get("asrInferenceMs", 0)) + float(
        window.get("mtInferenceMs", 0)
    )


def replacement_decision(
    baseline: str,
    candidate: str,
    min_ratio: float,
    max_ratio: float,
) -> tuple[bool, str, float | None]:
    baseline_normalized = normalize(baseline)
    candidate_normalized = normalize(candidate)
    if not candidate_normalized:
        return False, "candidate-empty", None
    if contains_artifact(candidate):
        return False, "candidate-artifact", None
    if contains_repetition(candidate):
        return False, "candidate-repetition", None
    if candidate_normalized == baseline_normalized:
        return False, "unchanged", 1.0
    if not baseline_normalized:
        return True, "fills-empty", None
    ratio = len(candidate_normalized) / len(baseline_normalized)
    if not min_ratio <= ratio <= max_ratio:
        return False, "implausible-length", ratio
    return True, "mechanical-gate", ratio


def single_run_per_clip(report: dict[str, Any], label: str) -> dict[str, dict[str, Any]]:
    runs: dict[str, dict[str, Any]] = {}
    for run in report.get("runs", []):
        clip_id = run["clipId"]
        if clip_id in runs:
            raise ValueError(
                f"{label} contains multiple runs for {clip_id}; filter it to one pipeline"
            )
        runs[clip_id] = run
    if not runs:
        raise ValueError(f"{label} contains no runs")
    return runs


def percentile(values: list[float], fraction: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = max(0, math.ceil(len(ordered) * fraction) - 1)
    return ordered[index]


def main() -> None:
    args = parse_args()
    baseline_report = json.loads(args.baseline.read_text())
    candidate_report = json.loads(args.candidate.read_text())
    baseline_runs = single_run_per_clip(baseline_report, "baseline")
    candidate_runs = single_run_per_clip(candidate_report, "candidate")
    clip_filter = {
        value.strip() for value in args.clip_filter.split(",") if value.strip()
    }
    if clip_filter:
        baseline_runs = {
            clip_id: run
            for clip_id, run in baseline_runs.items()
            if clip_id in clip_filter
        }
        candidate_runs = {
            clip_id: run
            for clip_id, run in candidate_runs.items()
            if clip_id in clip_filter
        }
        if not baseline_runs or not candidate_runs:
            raise ValueError("clip filter selected no comparable runs")
    if baseline_runs.keys() != candidate_runs.keys():
        missing = sorted(baseline_runs.keys() - candidate_runs.keys())
        extra = sorted(candidate_runs.keys() - baseline_runs.keys())
        raise ValueError(f"clip mismatch; missing={missing}, extra={extra}")

    comparisons: list[dict[str, Any]] = []
    latencies: list[float] = []
    accepted_latencies: list[float] = []
    reason_counts: dict[str, int] = {}
    changed = 0
    accepted = 0
    late = 0
    baseline_artifacts = 0
    candidate_artifacts = 0
    candidate_repetitions = 0

    for clip_id, baseline_run in baseline_runs.items():
        candidate_run = candidate_runs[clip_id]
        baseline_windows = baseline_run.get("windows", [])
        candidate_windows = candidate_run.get("windows", [])
        if len(baseline_windows) != len(candidate_windows):
            raise ValueError(
                f"window count mismatch for {clip_id}: "
                f"{len(baseline_windows)} != {len(candidate_windows)}"
            )
        for baseline_window, candidate_window in zip(
            baseline_windows, candidate_windows, strict=True
        ):
            coordinates = ("index", "startMs", "endMs")
            if any(
                baseline_window[key] != candidate_window[key] for key in coordinates
            ):
                raise ValueError(f"window alignment mismatch for {clip_id}")
            baseline_text = baseline_window.get("text", "").strip()
            candidate_text = candidate_window.get("text", "").strip()
            latency = window_latency_ms(candidate_window)
            gate_accepts, reason, length_ratio = replacement_decision(
                baseline_text,
                candidate_text,
                args.min_length_ratio,
                args.max_length_ratio,
            )
            within_delay = latency <= args.max_total_delay_ms
            accepted_here = gate_accepts and within_delay
            if normalize(baseline_text) != normalize(candidate_text):
                changed += 1
            if accepted_here:
                accepted += 1
                accepted_latencies.append(latency)
            if gate_accepts and not within_delay:
                late += 1
                reason = "candidate-late"
            reason_counts[reason] = reason_counts.get(reason, 0) + 1
            baseline_artifacts += int(contains_artifact(baseline_text))
            candidate_artifacts += int(contains_artifact(candidate_text))
            candidate_repetitions += int(contains_repetition(candidate_text))
            latencies.append(latency)
            comparisons.append(
                {
                    "clipId": clip_id,
                    "index": baseline_window["index"],
                    "startMs": baseline_window["startMs"],
                    "endMs": baseline_window["endMs"],
                    "baselineText": baseline_text,
                    "candidateText": candidate_text,
                    "candidateLatencyMs": round(latency, 3),
                    "lengthRatio": round(length_ratio, 3)
                    if length_ratio is not None
                    else None,
                    "accepted": accepted_here,
                    "reason": reason,
                }
            )

    baseline_first = next(iter(baseline_runs.values()))
    candidate_first = next(iter(candidate_runs.values()))
    summary = {
        "baselineModel": baseline_first.get("model"),
        "candidateModel": candidate_first.get("model"),
        "clipCount": len(baseline_runs),
        "windowCount": len(comparisons),
        "changedWindowCount": changed,
        "mechanicallyAcceptedWindowCount": accepted,
        "lateCandidateWindowCount": late,
        "baselineArtifactWindowCount": baseline_artifacts,
        "candidateArtifactWindowCount": candidate_artifacts,
        "candidateRepetitionWindowCount": candidate_repetitions,
        "candidateMedianLatencyMs": round(statistics.median(latencies), 3),
        "candidateP95LatencyMs": round(percentile(latencies, 0.95), 3),
        "acceptedMedianLatencyMs": round(statistics.median(accepted_latencies), 3)
        if accepted_latencies
        else None,
        "reasonCounts": dict(sorted(reason_counts.items())),
    }
    output = {
        "baselineReport": str(args.baseline),
        "candidateReport": str(args.candidate),
        "maxTotalDelayMs": args.max_total_delay_ms,
        "minLengthRatio": args.min_length_ratio,
        "maxLengthRatio": args.max_length_ratio,
        "summary": summary,
        "windows": comparisons,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n")
    print(args.output)
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
