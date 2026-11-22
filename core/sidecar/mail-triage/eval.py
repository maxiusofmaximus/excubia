"""Qué configuración de Laya separa phishing de correo legítimo, con 40 fichas sintéticas etiquetadas a mano.

Uso: uv run python eval.py [--checkpoint base|typed-decisions|multilingual|router ...]
Cambia una variable cada vez desde (ficha completa, preguntas actuales). Son indicios, no pruebas: 40 fichas
no bastan para afirmar nada fino.
"""

import argparse
import gc
import statistics
import sys
import time
from typing import NamedTuple

import torch

from engine import CHECKPOINTS, Engine
from eval_variants import FICHA_VARIANTS, QUESTION_VARIANTS
from ficha import render
from metrics import auc, best_threshold, precision_recall, separation
from synthetic import LABELED

sys.stdout.reconfigure(encoding="utf-8")


class Row(NamedTuple):
    config: str
    auc: float
    separation: float
    kind_top1: float
    action_top1: float
    precision: float  # con umbral 0.5
    recall: float
    best_f1: float
    best_threshold: float
    median_ms: float


def matrix():
    """Configuración base y, después, cada variante de ficha y de preguntas por separado."""
    yield "completa", "actual"
    yield from ((name, "actual") for name in FICHA_VARIANTS if name != "completa")
    yield from (("completa", name) for name in QUESTION_VARIANTS if name != "actual")


def top(probs: dict) -> str:
    return max(probs, key=probs.get)


def run(engine: Engine, ficha_name: str, question_name: str) -> Row:
    questions, positive = QUESTION_VARIANTS[question_name]
    labels = [case.phish for case in LABELED]
    scores, kind_hits, action_hits, times = [], 0, 0, []
    for case in LABELED:
        state = FICHA_VARIANTS[ficha_name](render(case.ficha))
        started = time.perf_counter()
        answers = engine.triage(state, questions)
        times.append((time.perf_counter() - started) * 1000)
        scores.append(answers["phish"][positive])
        kind_hits += top(answers["kind"]) == case.kind
        action_hits += top(answers["action"]) == case.action
    pos = [s for s, y in zip(scores, labels) if y]
    neg = [s for s, y in zip(scores, labels) if not y]
    precision, recall = precision_recall(scores, labels, 0.5)
    threshold, f1 = best_threshold(scores, labels)
    return Row(
        config=f"{ficha_name}/{question_name}",
        auc=auc(pos, neg),
        separation=separation(pos, neg),
        kind_top1=kind_hits / len(LABELED),
        action_top1=action_hits / len(LABELED),
        precision=precision,
        recall=recall,
        best_f1=f1,
        best_threshold=threshold,
        median_ms=statistics.median(times),
    )


def print_table(checkpoint: str, label: str, rows: list[Row]) -> None:
    print(f"\n== {checkpoint} ({label}) · {len(LABELED)} fichas · 12 phishing / 28 legítimos")
    print(f"{'config':<24}{'AUC':>7}{'sep':>7}{'kind@1':>8}{'act@1':>7}{'P@.5':>7}{'R@.5':>7}"
          f"{'thr*':>7}{'F1*':>7}{'ms':>7}")
    for r in rows:
        print(f"{r.config:<24}{r.auc:>7.3f}{r.separation:>7.3f}{r.kind_top1:>8.3f}{r.action_top1:>7.3f}"
              f"{r.precision:>7.3f}{r.recall:>7.3f}{r.best_threshold:>7.3f}{r.best_f1:>7.3f}{r.median_ms:>7.1f}")


def main() -> None:
    parser = argparse.ArgumentParser(description="Compara configuraciones de Laya con fichas sintéticas.")
    parser.add_argument("--checkpoint", action="append", choices=CHECKPOINTS,
                        help="se puede repetir; por defecto, todos")
    args = parser.parse_args()
    for checkpoint in args.checkpoint or CHECKPOINTS:
        engine = Engine.load(checkpoint)
        if not engine.loaded:
            print(f"\n== {checkpoint}: modelo no cargado")
            continue
        print_table(checkpoint, engine.label, [run(engine, f, q) for f, q in matrix()])
        del engine
        gc.collect()
        torch.cuda.empty_cache()


if __name__ == "__main__":
    main()
