"""Sintetiza la música y los efectos del vídeo. Solo biblioteca estándar de Python + ffmpeg.

Uso (desde docs/video):  python audio/make_audio.py
Escribe assets/audio/{music,whoosh-a,whoosh-b,pop,impact}.mp3. Es determinista (semilla fija).
Obra original de este repositorio, sin muestras de terceros. Licencia: CC0 1.0.
"""
import math
import os
import random
import struct
import subprocess
import wave

SR = 44100
BAR = 2.5  # 96 pulsos por minuto, compás de 4
BEAT = BAR / 4
BARS = 13  # 32,5 s
OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "audio")
TAU = 2 * math.pi

# (bajo, tríada) por acorde
CH = {
    "Dm": (73.42, (146.83, 174.61, 220.00)),
    "Bb": (58.27, (116.54, 146.83, 174.61)),
    "F": (87.31, (174.61, 220.00, 261.63)),
    "C": (65.41, (130.81, 164.81, 196.00)),
}
PROG = ["Dm", "Dm", "Dm", "Bb", "F", "C", "Dm", "Bb", "F", "C", "Dm", "Bb", "Dm"]


def save(name, left, right=None):
    right = right or left
    peak = max(max(map(abs, left)), max(map(abs, right)), 1e-9)
    gain = 0.7 / peak  # -3 dB de pico
    wav = os.path.join(OUT, name + ".wav")
    with wave.open(wav, "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(b"".join(
            struct.pack("<hh", int(l * gain * 32767), int(r * gain * 32767))
            for l, r in zip(left, right)))
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", wav, "-b:a", "192k",
                    os.path.join(OUT, name + ".mp3")], check=True)
    os.remove(wav)


def music():
    n = int(BARS * BAR * SR)
    L = [0.0] * n
    R = [0.0] * n
    rnd = random.Random(7)
    for bar, name in enumerate(PROG):
        bass, triad = CH[name]
        b0 = int(bar * BAR * SR)
        last = bar == BARS - 1
        blen = int(BAR * SR)
        # colchón: armónicos suaves con entrada y salida lentas; bajo con octava para altavoces pequeños
        for i in range(blen):
            t = i / SR
            env = min(1.0, t / 0.35, (BAR - t) / (1.2 if last else 0.35))
            s = 0.0
            for f in triad:
                for h, a in ((1, 1.0), (2, 0.45), (3, 0.22), (4, 0.1)):
                    s += a * math.sin(TAU * f * h * t)
            s *= 0.055
            s += 0.30 * math.sin(TAU * bass * t) + 0.16 * math.sin(TAU * bass * 2 * t)
            intro = min(1.0, (bar * BAR + t) / 2.0)  # fundido de entrada del tema
            L[b0 + i] += s * env * intro
            R[b0 + i] += s * env * intro
        if bar < 2 or last:
            continue  # la apertura y el acorde final van sin ritmo
        # arpegio pulsado en corcheas, alternando lados
        for k in range(8):
            f = triad[(0, 1, 2, 1, 0, 2, 1, 2)[k]] * 2
            start = b0 + int(k * BEAT / 2 * SR)
            pan = 0.35 if k % 2 else 0.65
            for i in range(int(0.45 * SR)):
                t = i / SR
                v = math.exp(-t * 7) * (math.sin(TAU * f * t) + 0.3 * math.sin(TAU * f * 2 * t)) * 0.13
                L[start + i] += v * pan
                R[start + i] += v * (1 - pan)
        # bombo suave en cada pulso
        for k in range(4):
            start = b0 + int(k * BEAT * SR)
            ph = 0.0
            for i in range(int(0.22 * SR)):
                t = i / SR
                ph += TAU * (48 + 90 * math.exp(-t * 40)) / SR
                v = math.sin(ph) * math.exp(-t * 16) * 0.5
                L[start + i] += v
                R[start + i] += v
        # platillo cerrado a contratiempo desde la segunda escena
        if bar >= 5:
            for k in range(4):
                start = b0 + int((k + 0.5) * BEAT * SR)
                prev = 0.0
                for i in range(int(0.06 * SR)):
                    x = rnd.uniform(-1, 1)
                    v = (x - prev) * math.exp(-i / SR * 70) * 0.07  # paso alto de un polo
                    prev = x
                    L[start + i] += v
                    R[start + i] += v
    save("music", L, R)


def whoosh(name, seed, top):
    rnd = random.Random(seed)
    n = int(0.7 * SR)
    out, y = [], 0.0
    for i in range(n):
        p = i / n
        env = math.sin(math.pi * p) ** 2
        k = 0.02 + top * env  # el filtro se abre y se cierra con la envolvente
        y += k * (rnd.uniform(-1, 1) - y)
        out.append(y * env)
    save(name, out)


def pop():
    n = int(0.09 * SR)
    out, ph = [], 0.0
    for i in range(n):
        t = i / SR
        ph += TAU * (220 + 500 * math.exp(-t * 60)) / SR
        out.append(math.sin(ph) * math.exp(-t * 45))
    save("pop", out)


def impact():
    rnd = random.Random(3)
    rise, tail = 1.2, 1.3
    out, y, ph = [], 0.0, 0.0
    for i in range(int((rise + tail) * SR)):
        t = i / SR
        if t < rise:
            p = t / rise
            y += (0.01 + 0.3 * p * p) * (rnd.uniform(-1, 1) - y)
            out.append(y * p * p * 0.5)
        else:
            u = t - rise
            ph += TAU * (42 + 60 * math.exp(-u * 25)) / SR
            y += 0.25 * (rnd.uniform(-1, 1) - y)
            out.append(math.sin(ph) * math.exp(-u * 3.5) + y * math.exp(-u * 12) * 0.5)
    save("impact", out)


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    music()
    whoosh("whoosh-a", 11, 0.35)
    whoosh("whoosh-b", 23, 0.2)
    pop()
    impact()
    print("audio listo en", os.path.normpath(OUT))
