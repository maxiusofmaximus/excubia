"""Whisper large-v3 speech worker: faster-whisper on CTranslate2, CUDA fp16 only, no CPU fallback.

Speaks the line protocol of asr_protocol.py. The host cuts silence with its own VAD before a request;
the prompt below is what makes the project's names come out right.
"""

import glob
import os
import sys
import traceback
from pathlib import Path

import ctranslate2
import numpy as np
from faster_whisper import WhisperModel

from asr_protocol import CUDA, RATE, send, serve

# Vocabulary prompt measured on the voice bench (VOZ-BANCO2): the names come out right with it.
# Its words are also the bench's keywords, so the measured gain is optimistic.
PROMPT = (
    "Conversación con un asistente de proyectos llamado Alfred. "
    "Vocabulario: Excubia, Alfred, delegar, aprobación, borrador, presupuesto, calendario, contacto, nota."
)
# faster-whisper's own thresholds for "this segment is silence".
NO_SPEECH_PROB = 0.6
AVG_LOGPROB = -1.0


def speech_text(segments):
    """The transcript, or "" when every segment looks like silence: Whisper invents phrases for silence."""
    if all(s.no_speech_prob > NO_SPEECH_PROB and s.avg_logprob < AVG_LOGPROB for s in segments):
        return ""
    return "".join(s.text for s in segments).strip()


def recognize(model, pcm):
    segments, _ = model.transcribe(
        pcm,
        language="es",
        beam_size=5,
        condition_on_previous_text=False,
        vad_filter=False,
        initial_prompt=PROMPT,
    )
    return speech_text(list(segments))


def _cuda_dlls():
    """The CUDA runtime comes from the nvidia-* wheels; CTranslate2 finds cuBLAS and cuDNN by DLL path."""
    site = Path(sys.executable).parent.parent / "Lib" / "site-packages"
    folders = glob.glob(str(site / "nvidia" / "*" / "bin"))
    for folder in folders:
        os.add_dll_directory(folder)
    os.environ["PATH"] = os.pathsep.join(folders + [os.environ["PATH"]])


def load_model(model_dir):
    """The model on CUDA, or None. A model that quietly runs on CPU is refused: it would be too slow."""
    _cuda_dlls()
    if ctranslate2.get_cuda_device_count() < 1:
        return None
    try:
        model = WhisperModel(model_dir, device="cuda", compute_type="float16")
    except Exception:
        traceback.print_exc()
        return None
    return model if model.model.device == "cuda" else None


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
