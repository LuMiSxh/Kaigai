#!/usr/bin/env python3
"""Prepare aligned Japanese-to-English FLEURS benchmark audio."""

from __future__ import annotations

import argparse
import csv
import json
import math
import random
import shutil
import subprocess
import tarfile
import urllib.request
import wave
from array import array
from pathlib import Path
from typing import Any


DATASET_REVISION = "70bb2e84b976b7e960aa89f1c648e09c59f894dd"
BASE_URL = f"https://huggingface.co/datasets/google/fleurs/resolve/{DATASET_REVISION}"
SAMPLE_RATE_HZ = 16_000


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--split", choices=["train", "dev", "test"], default="dev")
    parser.add_argument("--limit", type=int, default=24)
    parser.add_argument(
        "--snr-db",
        default="15,5",
        help="Comma-separated deterministic noise levels; empty keeps clean only.",
    )
    parser.add_argument(
        "--root", type=Path, default=Path("benchmarks/reference/fleurs-ja-en")
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("benchmarks/reference/generated/fleurs-ja-en.json"),
    )
    return parser.parse_args()


def download(url: str, destination: Path) -> None:
    if destination.is_file():
        return
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix(f"{destination.suffix}.part")
    print(f"download {url}", flush=True)
    with urllib.request.urlopen(url) as response, temporary.open("wb") as output:
        shutil.copyfileobj(response, output)
    temporary.replace(destination)


def read_table(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as source:
        rows = csv.reader(source, delimiter="\t")
        return [
            {
                "sentence_id": row[0],
                "audio_file": row[1],
                "text": row[2],
                "samples": row[5],
                "gender": row[6],
            }
            for row in rows
        ]


def select_parallel_rows(
    japanese: list[dict[str, str]], english: list[dict[str, str]], limit: int
) -> list[tuple[dict[str, str], str]]:
    english_by_id = {row["sentence_id"]: row["text"] for row in english}
    selected: list[tuple[dict[str, str], str]] = []
    seen: set[str] = set()
    for row in japanese:
        sentence_id = row["sentence_id"]
        if sentence_id in seen or sentence_id not in english_by_id:
            continue
        seen.add(sentence_id)
        selected.append((row, english_by_id[sentence_id]))
        if len(selected) == limit:
            break
    if len(selected) != limit:
        raise ValueError(f"requested {limit} aligned sentences, found {len(selected)}")
    return selected


def extract_audio(archive: Path, destination: Path) -> None:
    marker = destination / ".complete"
    if marker.is_file():
        return
    destination.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive) as source:
        source.extractall(destination, filter="data")
    marker.touch()


def find_audio(root: Path, filename: str) -> Path:
    matches = list(root.rglob(filename))
    if len(matches) != 1:
        raise ValueError(f"expected one extracted {filename}, found {len(matches)}")
    return matches[0]


def read_pcm(path: Path) -> tuple[wave._wave_params, array]:
    with wave.open(str(path), "rb") as source:
        params = source.getparams()
        if params.nchannels != 1 or params.sampwidth != 2 or params.framerate != SAMPLE_RATE_HZ:
            raise ValueError(
                f"{path} is not mono {SAMPLE_RATE_HZ} Hz 16-bit PCM: {params}"
            )
        samples = array("h")
        samples.frombytes(source.readframes(params.nframes))
    return params, samples


def ensure_pcm(source: Path, destination: Path) -> None:
    try:
        if destination.is_file():
            read_pcm(destination)
            return
    except (ValueError, wave.Error, EOFError):
        pass
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise RuntimeError("ffmpeg is required to convert FLEURS audio to PCM")
    destination.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            ffmpeg,
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            str(source),
            "-ac",
            "1",
            "-ar",
            str(SAMPLE_RATE_HZ),
            "-c:a",
            "pcm_s16le",
            str(destination),
        ],
        check=True,
    )


def write_pcm(path: Path, params: wave._wave_params, samples: array) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as output:
        output.setparams(params)
        output.writeframes(samples.tobytes())


def noisy_copy(source: Path, destination: Path, snr_db: float, seed: int) -> None:
    params, samples = read_pcm(source)
    signal_rms = math.sqrt(sum(float(value) ** 2 for value in samples) / len(samples))
    generator = random.Random(seed)
    state = 0.0
    noise = array("f")
    for _ in samples:
        state = 0.94 * state + 0.06 * generator.uniform(-1.0, 1.0)
        noise.append(state)
    noise_rms = math.sqrt(sum(float(value) ** 2 for value in noise) / len(noise))
    target_noise_rms = signal_rms / (10 ** (snr_db / 20))
    scale = target_noise_rms / max(noise_rms, 1e-9)
    mixed = array(
        "h",
        (
            max(-32_768, min(32_767, round(sample + noise_value * scale)))
            for sample, noise_value in zip(samples, noise, strict=True)
        ),
    )
    write_pcm(destination, params, mixed)


def clip_metadata(
    clip_id: str,
    audio_path: Path,
    source_text: str,
    reference_text: str,
    duration_seconds: float,
    gender: str,
    noise_profile: str,
    snr_db: float | None,
) -> dict[str, Any]:
    return {
        "id": clip_id,
        "category": "reference_translation",
        "lengthProfile": "reference",
        "speechDensity": "dense",
        "speakerProfile": "single",
        "speakerCount": 1,
        "overlapRisk": "low",
        "speaker": f"FLEURS {gender.lower()}",
        "title": "FLEURS Japanese-English parallel evaluation",
        "url": "https://huggingface.co/datasets/google/fleurs",
        "start": "00:00:00",
        "durationSeconds": duration_seconds,
        "audioPath": str(audio_path.resolve()),
        "sourceText": source_text,
        "referenceText": reference_text,
        "sourceDataset": "google/fleurs",
        "noiseProfile": noise_profile,
        "snrDb": snr_db,
    }


def main() -> None:
    args = parse_args()
    if args.limit < 1:
        raise ValueError("--limit must be positive")
    snr_levels = [float(value) for value in args.snr_db.split(",") if value.strip()]
    cache = args.root / "cache"
    japanese_table = cache / f"ja_jp-{args.split}.tsv"
    english_table = cache / f"en_us-{args.split}.tsv"
    archive = cache / f"ja_jp-audio-{args.split}.tar.gz"
    download(f"{BASE_URL}/data/ja_jp/{args.split}.tsv", japanese_table)
    download(f"{BASE_URL}/data/en_us/{args.split}.tsv", english_table)
    download(f"{BASE_URL}/data/ja_jp/audio/{args.split}.tar.gz", archive)
    extracted = args.root / "extracted" / args.split
    extract_audio(archive, extracted)

    selected = select_parallel_rows(
        read_table(japanese_table), read_table(english_table), args.limit
    )
    audio_dir = args.root / "audio"
    clips: list[dict[str, Any]] = []
    for index, (row, reference_text) in enumerate(selected):
        source = find_audio(extracted, row["audio_file"])
        clean = audio_dir / f"fleurs-{args.split}-{row['sentence_id']}-clean.wav"
        ensure_pcm(source, clean)
        params, _ = read_pcm(clean)
        duration = params.nframes / params.framerate
        base_id = f"fleurs-{args.split}-{row['sentence_id']}"
        clips.append(
            clip_metadata(
                f"{base_id}-clean",
                clean,
                row["text"],
                reference_text,
                duration,
                row["gender"],
                "clean",
                None,
            )
        )
        for snr_db in snr_levels:
            level = f"{abs(snr_db):g}".replace(".", "p")
            suffix = f"snr-{'m' if snr_db < 0 else ''}{level}"
            noisy = audio_dir / f"{base_id}-{suffix}.wav"
            if not noisy.is_file():
                noisy_copy(clean, noisy, snr_db, seed=index * 10_000 + round(snr_db * 10))
            clips.append(
                clip_metadata(
                    f"{base_id}-{suffix}",
                    noisy,
                    row["text"],
                    reference_text,
                    duration,
                    row["gender"],
                    "colored-noise",
                    snr_db,
                )
            )

    manifest = {
        "version": 1,
        "sampleRateHz": SAMPLE_RATE_HZ,
        "corpus": "FLEURS Japanese audio aligned to English parallel text",
        "datasetRevision": DATASET_REVISION,
        "license": "CC-BY-4.0",
        "clips": clips,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    print(args.output)
    print(f"{len(clips)} clips ({len(selected)} clean, {len(snr_levels)} noisy variants each)")


if __name__ == "__main__":
    main()
