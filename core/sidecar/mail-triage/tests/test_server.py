import http.client
import json
import threading

import pytest

from engine import Engine
from questions import QUESTIONS_ID
from server import MAX_BODY, TriageServer, main

SECRET = "CONTENIDO-SECRETO-DEL-CORREO"


class FakeEngine:
    label = "fake/laya@0.0.0"
    checkpoint = "typed-decisions"
    loaded = True
    device = "cpu"

    def __init__(self, error=None):
        self.error = error

    def health(self):
        return {"ok": True, "model": self.label, "checkpoint": self.checkpoint, "questions": QUESTIONS_ID,
                "device": self.device, "loaded": True, "warm": True}

    def triage(self, state):
        if self.error:
            raise self.error
        return {"phish": {"sí": 0.1, "no": 0.9}}


@pytest.fixture
def serve():
    servers = []

    def start(engine=None):
        server = TriageServer(engine or FakeEngine(), port=0)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        servers.append(server)
        return server.server_address[1]

    yield start
    for server in servers:
        server.shutdown()
        server.server_close()


def call(port, method, path, body=None, content_type="application/json", host=None):
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    headers = {"Host": host or f"127.0.0.1:{port}"}
    if content_type:
        headers["Content-Type"] = content_type
    data = body if body is None or isinstance(body, bytes) else json.dumps(body).encode()
    if data is not None:
        headers["Content-Length"] = str(len(data))
    conn.request(method, path, body=data, headers=headers)
    resp = conn.getresponse()
    raw = resp.read()
    conn.close()
    return resp.status, raw, json.loads(raw)


def valid_body(**extra):
    return {"state": {"from_address": "avisos@example.com", "subject": "Hola", **extra}}


def test_health(serve):
    port = serve()
    status, _, data = call(port, "GET", "/health")
    assert status == 200
    assert data == {"ok": True, "model": "fake/laya@0.0.0", "checkpoint": "typed-decisions",
                    "questions": QUESTIONS_ID, "device": "cpu", "loaded": True, "warm": True}


def test_triage_devuelve_respuestas(serve):
    port = serve()
    status, _, data = call(port, "POST", "/v1/triage", valid_body())
    assert status == 200
    assert data["questions"] == QUESTIONS_ID
    assert data["answers"] == {"phish": {"sí": 0.1, "no": 0.9}}
    assert isinstance(data["latency_ms"], int)


@pytest.mark.parametrize("host", ["evil.example:{port}", "127.0.0.1"])
def test_host_que_no_es_loopback_se_rechaza(serve, host):
    port = serve()
    status, _, data = call(port, "GET", "/health", host=host.format(port=port))
    assert status == 403
    assert data == {"error": "host_invalido"}


def test_localhost_es_aceptado(serve):
    port = serve()
    status, _, _ = call(port, "GET", "/health", host=f"localhost:{port}")
    assert status == 200


def test_json_roto_no_hace_eco(serve):
    port = serve()
    status, raw, data = call(port, "POST", "/v1/triage", b'{"state": ' + SECRET.encode())
    assert status == 400
    assert data == {"error": "json_invalido"}
    assert SECRET.encode() not in raw


def test_falta_state(serve):
    port = serve()
    status, _, data = call(port, "POST", "/v1/triage", {"ficha": {}})
    assert (status, data) == (400, {"error": "falta_state"})


def test_ficha_invalida(serve):
    port = serve()
    status, _, data = call(port, "POST", "/v1/triage", {"state": {"subject": "x"}})
    assert (status, data) == (400, {"error": "ficha_invalida"})


def test_content_type_incorrecto(serve):
    port = serve()
    status, _, data = call(port, "POST", "/v1/triage", valid_body(), content_type="text/plain")
    assert (status, data) == (400, {"error": "content_type"})


def test_cuerpo_demasiado_grande(serve):
    port = serve()
    status, _, data = call(port, "POST", "/v1/triage", b"x" * (MAX_BODY + 1))
    assert (status, data) == (400, {"error": "demasiado_grande"})


def test_ruta_desconocida(serve):
    port = serve()
    status, _, data = call(port, "GET", "/otra")
    assert (status, data) == (404, {"error": "no_encontrado"})


def test_error_del_modelo_no_expone_el_correo(serve, caplog):
    port = serve(FakeEngine(error=RuntimeError(SECRET)))
    status, raw, data = call(port, "POST", "/v1/triage", valid_body(preview=SECRET))
    assert (status, data) == (500, {"error": "model_error"})
    assert SECRET.encode() not in raw
    assert SECRET not in caplog.text


def test_el_log_no_guarda_el_cuerpo(serve, caplog):
    port = serve()
    with caplog.at_level("INFO", logger="mail-triage"):
        call(port, "POST", "/v1/triage", valid_body(preview=SECRET, subject=SECRET))
        call(port, "POST", "/v1/triage", b'{"state": "' + SECRET.encode())
    assert "POST /v1/triage 200" in caplog.text
    assert "POST /v1/triage 400" in caplog.text
    assert SECRET not in caplog.text


class ArranqueEngine(FakeEngine):
    def __init__(self, events):
        self.events = events

    def warm_up(self):
        self.events.append("calentamiento")


class RoturaAgent:
    device = None

    def predict(self, state, questions):
        raise RuntimeError(SECRET)


def arranca(monkeypatch, *argv):
    """Ejecuta main() sin quedarse escuchando y devuelve el orden de los pasos de arranque."""
    events = []
    monkeypatch.setattr(Engine, "load", lambda checkpoint: ArranqueEngine(events))
    monkeypatch.setattr(TriageServer, "serve_forever", lambda self: events.append("escucha"))
    main(["--port", "0", *argv])
    return events


def test_calienta_antes_de_escuchar(monkeypatch):
    assert arranca(monkeypatch) == ["calentamiento", "escucha"]


def test_no_warmup_no_calienta(monkeypatch):
    assert arranca(monkeypatch, "--no-warmup") == ["escucha"]


def test_si_el_calentamiento_falla_arranca_igual(monkeypatch, caplog):
    warm = []
    monkeypatch.setattr(Engine, "load", lambda checkpoint: Engine(checkpoint, RoturaAgent()))
    monkeypatch.setattr(TriageServer, "serve_forever", lambda self: warm.append(self.engine.warm))
    main(["--port", "0"])
    assert warm == [False]
    assert SECRET not in caplog.text
