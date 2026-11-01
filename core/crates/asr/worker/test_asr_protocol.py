"""Protocol tests with a fake recognizer: no GPU, no model. Run: python -m unittest test_asr_protocol"""

import contextlib
import io
import json
import struct
import unittest

from asr_protocol import MAX_SAMPLES, serve


def frame(request_id, samples):
    header = json.dumps({"id": request_id, "samples": len(samples)}) + "\n"
    return header.encode("ascii") + struct.pack(f"<{len(samples)}f", *samples)


def run(recognize, requests):
    protocol = io.BytesIO()
    with contextlib.redirect_stderr(io.StringIO()):
        serve(recognize, io.BytesIO(requests), protocol)
    return [json.loads(line) for line in protocol.getvalue().decode("ascii").splitlines()]


def length_text(pcm):
    return f"muestras {len(pcm)}"


class ServeTest(unittest.TestCase):
    def test_each_request_gets_its_own_text(self):
        replies = run(length_text, frame(0, [0.0] * 160) + frame(1, [0.0] * 3200))
        self.assertEqual([r["id"] for r in replies], [0, 1])
        self.assertEqual([r["text"] for r in replies], ["muestras 160", "muestras 3200"])

    def test_a_bad_header_is_refused_and_ends_the_stream(self):
        requests = b'{"id":1,"samples":0}\n' + frame(2, [0.0])
        self.assertEqual(run(length_text, requests), [{"error": "bad_request"}])

    def test_an_oversized_header_is_refused(self):
        requests = json.dumps({"id": 1, "samples": MAX_SAMPLES + 1}).encode("ascii") + b"\n"
        self.assertEqual(run(length_text, requests), [{"error": "bad_request"}])

    def test_a_failed_inference_answers_its_request_and_keeps_going(self):
        calls = []

        def flaky(pcm):
            calls.append(len(pcm))
            if len(calls) == 1:
                raise RuntimeError("boom")
            return "ok"

        replies = run(flaky, frame(0, [0.0]) + frame(1, [0.0]))
        self.assertEqual(replies[0], {"id": 0, "error": "inference_failed"})
        self.assertEqual(replies[1]["text"], "ok")

    def test_a_truncated_payload_ends_without_a_reply(self):
        requests = b'{"id":5,"samples":4}\n' + b"\x00\x00"
        self.assertEqual(run(length_text, requests), [])


if __name__ == "__main__":
    unittest.main()
