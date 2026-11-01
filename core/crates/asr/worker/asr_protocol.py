"""Line protocol of the speech workers; the host side is crates/asr/src/wire.rs.

Request: one JSON line {"id", "samples"}, then samples * 4 bytes of f32 little-endian PCM (16 kHz mono).
Reply: one JSON line {"id", "text", "ms"} or {"id", "error"}. Everything else goes to stderr.
"""

import json
import time
import traceback

import numpy as np

RATE = 16_000
CUDA = "CUDAExecutionProvider"
# The client sends at most 20 s; a larger header means a corrupt stream, not longer speech.
MAX_SAMPLES = 30 * RATE


def send(protocol, message):
    protocol.write((json.dumps(message) + "\n").encode("ascii"))
    protocol.flush()


def read_exact(stream, count):
    data = stream.read(count)
    if len(data) != count:
        raise EOFError
    return data


def transcribe(recognize, request_id, pcm):
    started = time.perf_counter()
    try:
        text = recognize(pcm)
    except Exception:
        # A failed inference answers its request; it must not kill the worker.
        traceback.print_exc()
        return {"id": request_id, "error": "inference_failed"}
    return {"id": request_id, "text": text, "ms": round((time.perf_counter() - started) * 1000)}


def serve(recognize, requests, protocol):
    """Answers requests until stdin closes or a request cannot be framed; `recognize(pcm)` returns text."""
    while True:
        line = requests.readline()
        if not line:
            return
        try:
            header = json.loads(line)
            request_id, count = header["id"], header["samples"]
            if not (isinstance(count, int) and 0 < count <= MAX_SAMPLES):
                raise ValueError(count)
        except (ValueError, KeyError, TypeError):
            # Without a valid header the payload length is unknown, so the stream cannot continue.
            send(protocol, {"error": "bad_request"})
            return
        try:
            payload = read_exact(requests, count * 4)
        except EOFError:
            return
        pcm = np.frombuffer(payload, dtype="<f4")
        send(protocol, transcribe(recognize, request_id, pcm))
