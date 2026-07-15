import sys
import unittest
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from benchmark_text import contains_artifact, contains_repetition, normalize


class BenchmarkTextTests(unittest.TestCase):
    def test_normalize_ignores_case_and_punctuation(self) -> None:
        self.assertEqual(normalize("Thank-you!"), "thankyou")

    def test_artifact_detects_embedded_outro(self) -> None:
        self.assertTrue(contains_artifact("Actual line. Thanks for watching!"))

    def test_repetition_detects_repeated_words(self) -> None:
        self.assertTrue(contains_repetition("yeah yeah yeah yeah yeah"))

    def test_repetition_keeps_normal_sentence(self) -> None:
        self.assertFalse(contains_repetition("this is a normal translated sentence"))


if __name__ == "__main__":
    unittest.main()
