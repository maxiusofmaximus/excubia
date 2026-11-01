"""Regenerates tones_2s5.opus: 1 s of 440 Hz, 0.5 s of silence, 1 s of 880 Hz at 16 kHz mono (Ogg Opus)."""

from pathlib import Path

import numpy as np
import soundfile as sf

RATE = 16_000


def tone(hz: float, seconds: float) -> np.ndarray:
    t = np.arange(int(RATE * seconds)) / RATE
    return 0.3 * np.sin(2 * np.pi * hz * t)


audio = np.concatenate([tone(440, 1.0), np.zeros(RATE // 2), tone(880, 1.0)]).astype(np.float32)
sf.write(Path(__file__).with_name("tones_2s5.opus"), audio, RATE, format="OGG", subtype="OPUS")
