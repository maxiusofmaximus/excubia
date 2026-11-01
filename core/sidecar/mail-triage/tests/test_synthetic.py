from synthetic import LABELED


def test_conjunto_de_evaluacion_tiene_12_phishing_y_28_legitimos():
    assert len(LABELED) == 40
    assert sum(case.phish for case in LABELED) == 12


def test_todas_las_fichas_usan_dominios_de_ejemplo():
    # Solo datos sintéticos: ningún remitente real puede colarse en el conjunto.
    for case in LABELED:
        assert case.ficha["from_address"].endswith(".example"), case.id
        assert case.ficha["checks"]["from_domain"].endswith(".example"), case.id
