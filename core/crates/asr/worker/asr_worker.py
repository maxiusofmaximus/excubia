"""Canary speech worker: loads the model once, then serves the line protocol of asr_protocol.py on stdin."""

import os

os.environ["HF_HUB_OFFLINE"] = "1"
for name in ("HF_TOKEN", "HUGGING_FACE_HUB_TOKEN"):
    os.environ.pop(name, None)

import sys  # noqa: E402

import numpy as np  # noqa: E402
import onnx_asr  # noqa: E402
import onnxruntime as ort  # noqa: E402

from asr_protocol import CUDA, RATE, send, serve  # noqa: E402

LANGUAGE = "es"


def session_providers(obj, seen):
    if id(obj) in seen:
        return []
    seen.add(id(obj))
    if callable(getattr(obj, "get_providers", None)):
        return [obj.get_providers()]
    found = []
    for value in getattr(obj, "__dict__", {}).values():
        found.extend(session_providers(value, seen))
    return found


def provider_error(model):
    """The error code to report, or None when every inference session runs on CUDA."""
    sessions = session_providers(model, set())
    if not sessions:
        return "provider_unverified"
    if any(names[0] != CUDA for names in sessions):
        return "no_cuda"
    return None


def recognize(model, pcm):
    return str(
        model.recognize(
            pcm,
            sample_rate=RATE,
            language=LANGUAGE,
            target_language=LANGUAGE,
            pnc=True,
        )
    )


def main():
    protocol = sys.stdout.buffer
    # Libraries print to stdout; the protocol owns the real one, so their output goes to stderr.
    sys.stdout = sys.stderr
    ort.preload_dlls()
    model = onnx_asr.load_model(
        "nemo-canary-1b-v2",
        path=sys.argv[1],
        providers=[(CUDA, {"device_id": 0}), "CPUExecutionProvider"],
    )
    error = provider_error(model)
    if error:
        send(protocol, {"error": error})
        return 1
    # The first inference pays for CUDA and cuDNN setup (~5 s measured); pay it before ready.
    recognize(model, np.zeros(RATE, dtype="<f4"))
    send(protocol, {"ready": True, "provider": CUDA})
    serve(lambda pcm: recognize(model, pcm), sys.stdin.buffer, protocol)
    return 0


if __name__ == "__main__":
    sys.exit(main())
