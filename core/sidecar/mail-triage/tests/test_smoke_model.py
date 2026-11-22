import pytest

from engine import DEFAULT_CHECKPOINT, SUBFOLDER, Engine, cached_snapshot
from questions import QUESTIONS

FICHA = {
    "untrusted": {
        "from_name": "Ejemplo",
        "from_address": "avisos@example.com",
        "subject": "Factura de octubre",
        "preview": "Te enviamos la factura de tu suscripción de octubre.",
    },
    "checks": {"auth": {"spf": "pass", "dkim": "pass", "dmarc": "pass"}},
    "deterministic": None,
    "link_count": 0,
    "contact_known": False,
    "preferences": None,
}


@pytest.mark.model
def test_modelo_real_devuelve_probabilidades():
    if cached_snapshot(SUBFOLDER[DEFAULT_CHECKPOINT]) is None:
        pytest.skip("checkpoint por defecto de Laya no está en caché")
    engine = Engine.load()
    assert engine.loaded
    engine.warm_up()
    assert engine.warm
    answers = engine.triage(FICHA)
    assert set(answers) == set(QUESTIONS)
    for qid, probs in answers.items():
        assert abs(sum(probs.values()) - 1.0) < 1e-3, qid
