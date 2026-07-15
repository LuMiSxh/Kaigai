# /// script
# dependencies = ["sacrebleu>=2.5,<3"]
# ///
"""Score a Kaigai benchmark report containing human reference translations."""

from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

from sacrebleu import corpus_chrf

from benchmark_text import contains_artifact, contains_repetition


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def slice_key(run: dict[str, Any]) -> tuple[str, str, str, str, str]:
    snr = "clean" if run.get("snrDb") is None else f"{run['snrDb']:g}dB"
    return (
        run["model"],
        run["backend"],
        run["task"],
        run.get("noiseProfile") or "unspecified",
        snr,
    )


def main() -> None:
    args = parse_args()
    report = json.loads(args.input.read_text())
    grouped: dict[tuple[str, str, str, str, str], list[dict[str, Any]]] = defaultdict(list)
    missing_references = []
    for run in report.get("runs", []):
        if not run.get("referenceText"):
            missing_references.append(run.get("clipId"))
            continue
        grouped[slice_key(run)].append(run)
    if missing_references:
        raise ValueError(
            f"report contains {len(missing_references)} runs without referenceText; "
            "use a reference corpus report"
        )
    if not grouped:
        raise ValueError("report contains no reference-backed runs")

    scores = []
    for key, runs in sorted(grouped.items()):
        hypotheses = [run.get("text", "") for run in runs]
        references = [run["referenceText"] for run in runs]
        chrf = corpus_chrf(hypotheses, [references], word_order=2)
        decisions = Counter(
            window["qualityDecision"]
            for run in runs
            for window in run.get("windows", [])
            if window.get("qualityDecision")
        )
        scores.append(
            {
                "model": key[0],
                "backend": key[1],
                "task": key[2],
                "noiseProfile": key[3],
                "snr": key[4],
                "clipCount": len(runs),
                "chrF2PlusPlus": round(chrf.score, 3),
                "emptyOutputCount": sum(not hypothesis.strip() for hypothesis in hypotheses),
                "artifactOutputCount": sum(contains_artifact(text) for text in hypotheses),
                "repetitionOutputCount": sum(contains_repetition(text) for text in hypotheses),
                "qualityDecisionCounts": dict(sorted(decisions.items())),
                "averageRealtimeFactor": round(
                    sum(run["realtimeFactor"] for run in runs) / len(runs), 4
                ),
            }
        )

    output = {
        "sourceReport": str(args.input),
        "metric": "chrF2++ (sacreBLEU chrF with word_order=2)",
        "scores": scores,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n")
    print(args.output)
    print(json.dumps(scores, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
