"""Métricas de separación para eval.py. Funciones puras: sin modelo ni red."""


def auc(pos: list[float], neg: list[float]) -> float:
    """Probabilidad de que un positivo puntúe más que un negativo (empates cuentan medio)."""
    wins = sum((p > n) + 0.5 * (p == n) for p in pos for n in neg)
    return wins / (len(pos) * len(neg))


def separation(pos: list[float], neg: list[float]) -> float:
    return sum(pos) / len(pos) - sum(neg) / len(neg)


def precision_recall(scores: list[float], labels: list[bool], threshold: float) -> tuple[float, float]:
    flagged = [y for s, y in zip(scores, labels) if s >= threshold]
    tp = sum(flagged)
    fn = sum(labels) - tp
    precision = tp / len(flagged) if flagged else 0.0
    recall = tp / (tp + fn) if tp + fn else 0.0
    return precision, recall


def best_threshold(scores: list[float], labels: list[bool]) -> tuple[float, float]:
    """Umbral (entre las puntuaciones observadas) con mejor F1, y ese F1."""
    best = (0.0, 0.0)
    for threshold in sorted(set(scores)):
        precision, recall = precision_recall(scores, labels, threshold)
        f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0
        if f1 > best[1]:
            best = (threshold, f1)
    return best
