"""Carga de Laya y traducción de sus respuestas al formato de /v1/triage."""

import logging
import os
import threading
import time
from importlib.metadata import version

import laya
from huggingface_hub import snapshot_download

from ficha import render
from questions import QUESTIONS, QUESTIONS_ID

log = logging.getLogger("mail-triage")
REPO = "convaiinnovations/laya"
WEIGHTS = ["rl_agent_config.json", "model.safetensors", "tokenizer/*", "encoder/*"]
# Subcarpeta de REPO por checkpoint; "router" deja que laya elija inglés o multilingüe por idioma.
SUBFOLDER = {"base": None, "typed-decisions": "typed-decisions", "multilingual": "multilingual"}
CHECKPOINTS = (*SUBFOLDER, "router")
# Medido con eval.py (40 fichas sintéticas): el único que separa phishing; ver README.
DEFAULT_CHECKPOINT = "typed-decisions"
# Ficha inventada (dominio example.*) para el calentamiento; pasa por render() como una ficha real.
WARMUP_FICHA = {
    "from_name": "Ejemplo",
    "from_address": "avisos@example.com",
    "subject": "Factura de octubre",
    "preview": "Te enviamos la factura de tu suscripción de octubre.",
    "checks": {"auth": {"spf": "pass", "dkim": "pass", "dmarc": "pass"}},
    "link_count": 0,
    "contact_known": False,
}


def cached_snapshot(subfolder: str | None = None) -> str | None:
    """Carpeta local con el checkpoint si ya está en caché; None si hay que descargarlo."""
    prefix = f"{subfolder}/" if subfolder else ""
    try:
        local = snapshot_download(REPO, allow_patterns=[prefix + p for p in WEIGHTS], local_files_only=True)
    except FileNotFoundError:
        return None
    # Con la raíz en caché, snapshot_download no falla aunque falte la subcarpeta: se mira el archivo principal.
    return local if os.path.exists(os.path.join(local, prefix, "model.safetensors")) else None


def probabilities(spec: dict, answer: dict) -> dict:
    """Probabilidad por opción (suma 1) para una pregunta de Laya."""
    if spec["type"] == "noul":
        yes = answer["noul"]
        return {"sí": yes, "no": round(1 - yes, 4)}
    if spec["type"] == "score":
        return {level: answer["probabilities"][str(i)] for i, level in enumerate(spec["criteria"])}
    return answer["probabilities"]


def _build(checkpoint: str):
    if checkpoint == "router":
        return laya.Router()
    sub = SUBFOLDER[checkpoint]
    local = cached_snapshot(sub)
    log.info("checkpoint %s: %s", checkpoint, "en caché (sin descarga)" if local else "no está en caché: se descarga")
    return laya.load(local or REPO, subfolder=sub)


def _source(checkpoint: str) -> str:
    if checkpoint == "router":
        return "laya.Router"
    sub = SUBFOLDER[checkpoint]
    return f"{REPO}/{sub}" if sub else REPO


class Engine:
    """Un único modelo cargado; las llamadas se serializan porque comparten la GPU."""

    def __init__(self, checkpoint: str = DEFAULT_CHECKPOINT, agent=None):
        self.checkpoint = checkpoint
        self._agent = agent
        self._lock = threading.Lock()
        self.warm = False

    @classmethod
    def load(cls, checkpoint: str = DEFAULT_CHECKPOINT) -> "Engine":
        """Precarga el checkpoint; si falla, el servidor sigue arriba con loaded=false (fallo cerrado)."""
        try:
            return cls(checkpoint, _build(checkpoint))
        except Exception as exc:  # el error del modelo puede traer texto; solo se registra el tipo
            log.error("modelo no cargado: %s", type(exc).__name__)
            return cls(checkpoint, None)

    @property
    def label(self) -> str:
        return f"{_source(self.checkpoint)}@{version('laya')}"

    @property
    def loaded(self) -> bool:
        return self._agent is not None

    @property
    def device(self) -> str | None:
        # laya.Router no tiene dispositivo propio (None): cada checkpoint elige el suyo al cargarse.
        device = self._agent.device if self.loaded else None
        return device.type if device is not None else None

    def health(self) -> dict:
        return {
            "ok": self.loaded,
            "model": self.label,
            "checkpoint": self.checkpoint,
            "questions": QUESTIONS_ID,
            "device": self.device,
            "loaded": self.loaded,
            "warm": self.warm,
        }

    def triage(self, state: dict, questions: dict = QUESTIONS) -> dict:
        if not self.loaded:
            raise RuntimeError("modelo no cargado")
        with self._lock:
            raw = self._agent.predict(state, questions)["answers"]
        return {qid: probabilities(spec, raw[qid]) for qid, spec in questions.items()}

    def warm_up(self) -> None:
        """Una inferencia con una ficha sintética: paga la carga perezosa de CUDA antes de escuchar."""
        started = time.perf_counter()
        try:
            self.triage(render(WARMUP_FICHA))
        except Exception as exc:  # el error del modelo puede traer texto del correo: solo se registra el tipo
            log.warning("calentamiento fallido: %s", type(exc).__name__)
            return
        self.warm = True
        log.info("calentamiento en %.0f ms", (time.perf_counter() - started) * 1000)
