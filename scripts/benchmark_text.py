"""Shared text diagnostics for Kaigai benchmark reports."""

from __future__ import annotations

import re


STRONG_ARTIFACTS = (
    "thank you for watching",
    "thanks for watching",
    "see you next time",
    "please subscribe",
    "like and subscribe",
    "don't forget to like my video",
    "subscribe to my channel",
    "thank you for your viewing",
    "translated by",
    "i'm not sure what this phrase means",
    "if you provide more context",
    "i can try to help with a more accurate translation",
    "here is the correct translation",
    "sorry for the bad translation",
    "back with another video",
    "ご視聴ありがとうございました",
    "ご視聴ありがとうございます",
    "チャンネル登録お願いします",
)


def normalize(text: str) -> str:
    return "".join(character.lower() for character in text if character.isalnum())


def contains_artifact(text: str) -> bool:
    lowered = text.casefold()
    return any(artifact.casefold() in lowered for artifact in STRONG_ARTIFACTS)


def contains_repetition(text: str) -> bool:
    words = re.findall(r"[\w']+", text.casefold())
    if any(
        words[index : index + 4] == [words[index]] * 4
        for index in range(max(0, len(words) - 3))
    ):
        return True
    squashed = normalize(text)
    for unit_length in range(2, min(40, len(squashed) // 4) + 1):
        unit = squashed[:unit_length]
        if squashed.startswith(unit * 4):
            return True
    return False
