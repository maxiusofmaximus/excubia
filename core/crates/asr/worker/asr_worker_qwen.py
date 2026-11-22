"""Qwen3-ASR-1.7B speech worker: qwen-asr on transformers, bf16 on CUDA only, no CPU fallback.

Speaks the line protocol of asr_protocol.py. The host cuts silence with its own VAD before a request.
"""

import sys
import traceback
from itertools import groupby

import numpy as np
import torch
from qwen_asr import Qwen3ASRModel

from asr_protocol import CUDA, RATE, send, serve

# The context measured as «ciego» on the voice bench: one sentence, no vocabulary list.
CONTEXT = "Conversación con un asistente de proyectos llamado Alfred."
# Qwen answers silence or noise with a stray character or one word looping; "sí" and "no" are real replies.
MIN_LETTERS = 2
LOOP = 4


def speech_text(text):
    """The transcript, or "" when it is the stray text Qwen answers to silence or noise."""
    text = text.strip()
    if sum(c.isalnum() for c in text) < MIN_LETTERS:
        return ""
    if any(len(list(run)) >= LOOP for _, run in groupby(text.lower().split())):
        return ""
    return text


def recognize(model, pcm):
    result = model.transcribe(audio=(pcm, RATE), context=CONTEXT, language="Spanish")[0]
    return speech_text(result.text)


def load_model(model_dir):
    """The model on CUDA, or None. A model that quietly runs on CPU is refused: it would be too slow."""
    if not torch.cuda.is_available():
        return None
    try:
        model = Qwen3ASRModel.from_pretrained(
            model_dir,
            dtype=torch.bfloat16,
            device_map="cuda:0",
            max_inference_batch_size=1,
            max_new_tokens=256,
        )
    except Exception:
        traceback.print_exc()
        return None
    return model if model.model.device.type == "cuda" else None


def main():
    protocol = sys.stdout.buffer
    # Libraries print to stdout; the protocol owns the real one, so their output goes to stderr.
    sys.stdout = sys.stderr
    model = load_model(sys.argv[1])
    if model is None:
        send(protocol, {"error": "no_cuda"})
        return 1
    # The first inference pays for CUDA setup; pay it before ready.
    recognize(model, np.zeros(RATE, dtype="<f4"))
    send(protocol, {"ready": True, "provider": CUDA})
    serve(lambda pcm: recognize(model, pcm), sys.stdin.buffer, protocol)
    return 0


if __name__ == "__main__":
    sys.exit(main())
