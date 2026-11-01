import os
from types import SimpleNamespace

import engine as engine_module
from engine import Engine, cached_snapshot, probabilities
from questions import QUESTIONS


class FakeAgent:
    device = SimpleNamespace(type="cpu")

    def __init__(self, raw):
        self.raw = raw
        self.seen = None

    def predict(self, state, questions):
        self.seen = questions
        return {"answers": self.raw}


class FallaAgent:
    device = None

    def predict(self, state, questions):
        raise RuntimeError("texto del correo")


def respuestas_completas():
    return {qid: ({"noul": 0.5} if spec["type"] == "noul" else {"probabilities": {"0": 0.5, "1": 0.5, "2": 0.5}})
            for qid, spec in QUESTIONS.items()}


def test_noul_se_convierte_en_si_y_no():
    assert probabilities({"type": "noul"}, {"noul": 0.8}) == {"sí": 0.8, "no": 0.2}


def test_triage_usa_las_preguntas_que_se_le_pasan():
    custom = {"phish": {"type": "choice", "instructions": "x", "criteria": {"phishing": "a", "legitimo": "b"}}}
    agent = FakeAgent({"phish": {"probabilities": {"phishing": 0.7, "legitimo": 0.3}}})
    engine = Engine("typed-decisions", agent)
    assert engine.triage({"untrusted": {}}, custom) == {"phish": {"phishing": 0.7, "legitimo": 0.3}}
    assert agent.seen is custom


def test_triage_por_defecto_usa_las_preguntas_del_triaje():
    agent = FakeAgent(respuestas_completas())
    Engine("typed-decisions", agent).triage({})
    assert agent.seen is QUESTIONS


def test_warm_up_hace_una_inferencia_y_marca_warm():
    agent = FakeAgent(respuestas_completas())
    engine = Engine("typed-decisions", agent)
    engine.warm_up()
    assert engine.warm
    assert agent.seen is QUESTIONS


def test_warm_up_con_fallo_no_lanza_y_no_marca_warm(caplog):
    engine = Engine("typed-decisions", FallaAgent())
    engine.warm_up()
    assert not engine.warm
    assert "texto del correo" not in caplog.text


def test_router_sin_dispositivo_propio_no_rompe_device():
    assert Engine("router", SimpleNamespace(device=None)).device is None


def test_sin_modelo_el_triaje_falla_cerrado():
    engine = Engine("base", None)
    assert not engine.loaded
    assert engine.health()["checkpoint"] == "base"


def test_load_con_error_queda_no_cargado(monkeypatch):
    def broken(checkpoint):
        raise OSError("sin red")

    monkeypatch.setattr(engine_module, "_build", broken)
    engine = Engine.load("multilingual")
    assert (engine.checkpoint, engine.loaded) == ("multilingual", False)


def test_cached_snapshot_no_da_por_bueno_una_subcarpeta_ausente(monkeypatch, tmp_path):
    (tmp_path / "model.safetensors").write_bytes(b"")  # solo la raíz está en caché
    monkeypatch.setattr(engine_module, "snapshot_download", lambda *a, **k: str(tmp_path))
    assert cached_snapshot("typed-decisions") is None
    assert cached_snapshot(None) == str(tmp_path)


def test_cached_snapshot_pide_los_patrones_de_la_subcarpeta(monkeypatch, tmp_path):
    seen = {}

    def fake(repo, allow_patterns, local_files_only):
        seen["patterns"] = allow_patterns
        os.makedirs(tmp_path / "multilingual")
        (tmp_path / "multilingual" / "model.safetensors").write_bytes(b"")
        return str(tmp_path)

    monkeypatch.setattr(engine_module, "snapshot_download", fake)
    assert cached_snapshot("multilingual") == str(tmp_path)
    assert "multilingual/model.safetensors" in seen["patterns"]
    assert "multilingual/tokenizer/*" in seen["patterns"]
