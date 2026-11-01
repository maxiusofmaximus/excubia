"""Whisper worker tests with a fake model: the silence filter and the decoding options. No GPU.

Run with the faster-whisper venv: python -m unittest test_asr_worker_whisper
"""

import collections
import unittest

import numpy as np

from asr_worker_whisper import PROMPT, recognize, speech_text

Segment = collections.namedtuple("Segment", "text no_speech_prob avg_logprob")


class FakeModel:
    def __init__(self, segments):
        self.segments = segments
        self.options = None

    def transcribe(self, pcm, **options):
        self.options = options
        return iter(self.segments), None


class SpeechTextTest(unittest.TestCase):
    def test_a_phrase_invented_for_silence_is_dropped(self):
        self.assertEqual(speech_text([Segment(" Gracias.", 0.9, -1.4)]), "")

    def test_real_speech_is_kept_and_stripped(self):
        self.assertEqual(speech_text([Segment(" Hola Alfred.", 0.1, -0.3)]), "Hola Alfred.")

    def test_one_speech_segment_keeps_the_whole_text(self):
        segments = [Segment(" Gracias.", 0.9, -1.4), Segment(" Delega la nota.", 0.1, -0.3)]
        self.assertEqual(speech_text(segments), "Gracias. Delega la nota.")

    def test_low_log_probability_alone_is_not_silence(self):
        self.assertEqual(speech_text([Segment(" Excubia.", 0.1, -1.5)]), "Excubia.")

    def test_no_segments_is_empty(self):
        self.assertEqual(speech_text([]), "")


class RecognizeTest(unittest.TestCase):
    def test_asks_for_spanish_with_the_project_prompt_and_no_vad(self):
        model = FakeModel([Segment(" Hola.", 0.1, -0.2)])
        self.assertEqual(recognize(model, np.zeros(16_000, dtype=np.float32)), "Hola.")
        self.assertEqual(model.options["language"], "es")
        self.assertEqual(model.options["initial_prompt"], PROMPT)
        self.assertEqual(model.options["beam_size"], 5)
        self.assertFalse(model.options["condition_on_previous_text"])
        self.assertFalse(model.options["vad_filter"])


if __name__ == "__main__":
    unittest.main()
