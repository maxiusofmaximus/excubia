"""Qwen worker tests with a fake model: the noise filter and the call options. No GPU.

Run with the qwen-asr venv: python -m unittest test_asr_worker_qwen
"""

import collections
import unittest

import numpy as np

from asr_protocol import RATE
from asr_worker_qwen import CONTEXT, recognize, speech_text

Result = collections.namedtuple("Result", "text")


class FakeModel:
    def __init__(self, text):
        self.text = text
        self.options = None

    def transcribe(self, audio, **options):
        self.options = options
        self.audio = audio
        return [Result(self.text)]


class SpeechTextTest(unittest.TestCase):
    def test_a_stray_character_is_dropped(self):
        self.assertEqual(speech_text(" ."), "")

    def test_a_looping_word_is_dropped(self):
        self.assertEqual(speech_text("no no no no"), "")

    def test_two_letters_are_a_real_reply(self):
        self.assertEqual(speech_text(" Sí. "), "Sí.")
        self.assertEqual(speech_text("No"), "No")

    def test_a_word_repeated_apart_is_kept(self):
        self.assertEqual(speech_text("Delega la nota de la agenda de hoy"), "Delega la nota de la agenda de hoy")

    def test_empty_is_empty(self):
        self.assertEqual(speech_text(""), "")


class RecognizeTest(unittest.TestCase):
    def test_asks_for_spanish_with_the_context_and_no_vocabulary(self):
        model = FakeModel(" Hola Alfred. ")
        pcm = np.zeros(RATE, dtype=np.float32)
        self.assertEqual(recognize(model, pcm), "Hola Alfred.")
        self.assertIs(model.audio[0], pcm)
        self.assertEqual(model.audio[1], RATE)
        self.assertEqual(model.options["language"], "Spanish")
        self.assertEqual(model.options["context"], CONTEXT)

    def test_silence_comes_back_empty(self):
        self.assertEqual(recognize(FakeModel("."), np.zeros(RATE, dtype=np.float32)), "")


if __name__ == "__main__":
    unittest.main()
