# /// script
# dependencies = [
#   "accelerate>=1.2,<2",
#   "peft>=0.14,<1",
#   "soundfile>=0.12,<1",
#   "torch>=2.5,<3",
#   "transformers>=4.48,<5",
# ]
# ///
"""Run a decoder-LoRA Japanese-to-English Whisper training probe locally."""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import soundfile
import torch
from peft import LoraConfig, get_peft_model
from torch.utils.data import Dataset
from transformers import (
    Seq2SeqTrainer,
    Seq2SeqTrainingArguments,
    WhisperForConditionalGeneration,
    WhisperProcessor,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--train-manifest", required=True, type=Path)
    parser.add_argument("--eval-manifest", required=True, type=Path)
    parser.add_argument("--output", type=Path, default=Path("training/whisper-medium-ja-en-lora"))
    parser.add_argument("--base-model", default="openai/whisper-medium")
    parser.add_argument("--max-steps", type=int, default=100)
    parser.add_argument("--learning-rate", type=float, default=1e-4)
    parser.add_argument("--gradient-accumulation", type=int, default=8)
    parser.add_argument("--save-steps", type=int, default=50)
    parser.add_argument("--seed", type=int, default=17)
    parser.add_argument("--merge-output", type=Path)
    return parser.parse_args()


class ManifestDataset(Dataset[dict[str, Any]]):
    def __init__(self, manifest_path: Path, processor: WhisperProcessor) -> None:
        manifest = json.loads(manifest_path.read_text())
        self.items = [clip for clip in manifest["clips"] if clip.get("referenceText")]
        if not self.items:
            raise ValueError(f"{manifest_path} contains no referenceText entries")
        self.processor = processor

    def __len__(self) -> int:
        return len(self.items)

    def __getitem__(self, index: int) -> dict[str, Any]:
        item = self.items[index]
        audio, sample_rate = soundfile.read(item["audioPath"], dtype="float32")
        if sample_rate != 16_000 or audio.ndim != 1:
            raise ValueError(f"{item['audioPath']} must be mono 16 kHz audio")
        input_features = self.processor.feature_extractor(
            audio, sampling_rate=sample_rate
        ).input_features[0]
        labels = self.processor.tokenizer(item["referenceText"]).input_ids
        return {"input_features": input_features, "labels": labels}


@dataclass
class SpeechTranslationCollator:
    processor: WhisperProcessor
    decoder_start_token_id: int

    def __call__(self, features: list[dict[str, Any]]) -> dict[str, torch.Tensor]:
        inputs = self.processor.feature_extractor.pad(
            [{"input_features": item["input_features"]} for item in features],
            return_tensors="pt",
        )
        labels = self.processor.tokenizer.pad(
            [{"input_ids": item["labels"]} for item in features],
            return_tensors="pt",
        )
        label_ids = labels["input_ids"].masked_fill(labels.attention_mask.ne(1), -100)
        if torch.all(label_ids[:, 0] == self.decoder_start_token_id):
            label_ids = label_ids[:, 1:]
        inputs["labels"] = label_ids
        return inputs


def choose_device() -> tuple[str, torch.dtype]:
    if torch.backends.mps.is_available():
        return "mps", torch.float16
    if torch.cuda.is_available():
        return "cuda", torch.float16
    return "cpu", torch.float32


def main() -> None:
    args = parse_args()
    if args.max_steps < 1:
        raise ValueError("--max-steps must be positive")
    if args.gradient_accumulation < 1:
        raise ValueError("--gradient-accumulation must be positive")
    if args.save_steps < 1:
        raise ValueError("--save-steps must be positive")
    if args.learning_rate <= 0:
        raise ValueError("--learning-rate must be positive")
    device, dtype = choose_device()
    processor = WhisperProcessor.from_pretrained(
        args.base_model, language="Japanese", task="translate"
    )
    model = WhisperForConditionalGeneration.from_pretrained(
        args.base_model,
        dtype=dtype,
        low_cpu_mem_usage=True,
    )
    model.config.use_cache = False
    model.generation_config.language = "ja"
    model.generation_config.task = "translate"
    model.freeze_encoder()
    model.gradient_checkpointing_enable()
    model = get_peft_model(
        model,
        LoraConfig(
            r=16,
            lora_alpha=32,
            lora_dropout=0.05,
            bias="none",
            target_modules=r".*decoder\.layers\.\d+\.(self_attn|encoder_attn)\.(q_proj|v_proj)$",
        ),
    )
    model.print_trainable_parameters()

    train_dataset = ManifestDataset(args.train_manifest, processor)
    eval_dataset = ManifestDataset(args.eval_manifest, processor)
    collator = SpeechTranslationCollator(processor, model.config.decoder_start_token_id)
    training_args = Seq2SeqTrainingArguments(
        output_dir=str(args.output),
        per_device_train_batch_size=1,
        per_device_eval_batch_size=1,
        gradient_accumulation_steps=args.gradient_accumulation,
        learning_rate=args.learning_rate,
        max_steps=args.max_steps,
        warmup_steps=min(20, max(1, args.max_steps // 10)),
        eval_strategy="steps",
        eval_steps=args.save_steps,
        save_steps=args.save_steps,
        logging_steps=5,
        save_total_limit=2,
        gradient_checkpointing=True,
        gradient_checkpointing_kwargs={"use_reentrant": False},
        fp16=device != "cpu",
        dataloader_pin_memory=False,
        remove_unused_columns=False,
        label_names=["labels"],
        report_to=[],
        seed=args.seed,
        data_seed=args.seed,
    )
    trainer = Seq2SeqTrainer(
        model=model,
        args=training_args,
        train_dataset=train_dataset,
        eval_dataset=eval_dataset,
        data_collator=collator,
        processing_class=processor,
    )
    trainer.train()
    trainer.save_model()
    processor.save_pretrained(args.output)

    if args.merge_output is not None:
        merged = model.merge_and_unload()
        merged.save_pretrained(args.merge_output, safe_serialization=True)
        processor.save_pretrained(args.merge_output)


if __name__ == "__main__":
    main()
