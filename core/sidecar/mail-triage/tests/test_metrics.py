import pytest

from metrics import auc, best_threshold, precision_recall, separation


def test_auc_separable_es_uno():
    assert auc([0.9, 0.8], [0.1, 0.2]) == 1.0


def test_auc_invertido_es_cero():
    assert auc([0.1], [0.9]) == 0.0


def test_auc_empates_cuentan_medio():
    assert auc([0.5, 0.5], [0.5]) == 0.5


def test_separacion_es_diferencia_de_medias():
    assert separation([0.9, 0.7], [0.2, 0.4]) == pytest.approx(0.5)


def test_precision_recall_con_umbral():
    scores = [0.9, 0.6, 0.4, 0.1]
    labels = [True, False, True, False]
    assert precision_recall(scores, labels, 0.5) == (0.5, 0.5)


def test_mejor_umbral_separa_perfecto():
    threshold, f1 = best_threshold([0.9, 0.8, 0.2, 0.1], [True, True, False, False])
    assert f1 == 1.0
    assert threshold == 0.8
