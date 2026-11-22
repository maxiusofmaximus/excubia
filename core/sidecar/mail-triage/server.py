"""Servidor HTTP del triaje en 127.0.0.1 (solo biblioteca estándar). Modo sombra: solo registra y responde."""

import argparse
import json
import logging
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit

from engine import CHECKPOINTS, DEFAULT_CHECKPOINT, Engine
from ficha import render
from questions import QUESTIONS_ID

HOST = "127.0.0.1"
DEFAULT_PORT = 8766
MAX_BODY = 32 * 1024
log = logging.getLogger("mail-triage")


class TriageServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, engine: Engine, port: int = DEFAULT_PORT):
        self.engine = engine
        super().__init__((HOST, port), TriageHandler)

    def allowed_hosts(self) -> set[str]:
        port = self.server_address[1]
        return {f"{HOST}:{port}", f"localhost:{port}"}

    def handle_error(self, request, client_address) -> None:
        # El traceback puede citar el contenido del cuerpo: solo se registra el tipo.
        log.error("error de conexión: %s", sys.exc_info()[0].__name__)


class TriageHandler(BaseHTTPRequestHandler):
    server_version = "mail-triage"
    sys_version = ""
    timeout = 5  # un cliente que no envía no retiene el hilo

    def parse_request(self) -> bool:
        self._t0 = time.perf_counter()
        return super().parse_request()

    def log_message(self, format: str, *args: object) -> None:
        # El log lo escribe _reply: método, ruta, estado y latencia, nunca el cuerpo.
        pass

    def do_GET(self) -> None:
        if not self._host_ok():
            return
        if self.path != "/health":
            return self._reply(404, {"error": "no_encontrado"})
        self._reply(200, self.server.engine.health())

    def do_POST(self) -> None:
        if not self._host_ok():
            return
        if self.path != "/v1/triage":
            return self._reply(404, {"error": "no_encontrado"})
        state = self._read_state()
        if isinstance(state, str):
            return self._reply(400, {"error": state})
        started = time.perf_counter()
        try:
            answers = self.server.engine.triage(state)
        except Exception:  # el mensaje del modelo puede contener el correo: no se devuelve ni registra
            return self._reply(500, {"error": "model_error"})
        self._reply(200, {
            "model": self.server.engine.label,
            "questions": QUESTIONS_ID,
            "answers": answers,
            "latency_ms": round((time.perf_counter() - started) * 1000),
        })

    def _host_ok(self) -> bool:
        if self.headers.get("Host") in self.server.allowed_hosts():
            return True
        self._reply(403, {"error": "host_invalido"})
        return False

    def _read_state(self) -> dict | str:
        """Estado para Laya, o el código de error si el cuerpo no sirve. Nunca hace eco del cuerpo."""
        length = self.headers.get("Content-Length", "")
        if not length.isdigit():
            return "longitud"
        size = int(length)
        if self.headers.get_content_type() != "application/json":
            self._discard(size)
            return "content_type"
        if size > MAX_BODY:
            self._discard(size)
            return "demasiado_grande"
        try:
            body = json.loads(self.rfile.read(size))
        except ValueError:
            return "json_invalido"
        if not isinstance(body, dict) or "state" not in body:
            return "falta_state"
        return render(body["state"])

    def _discard(self, size: int) -> None:
        # Responder con el cuerpo sin leer hace que Windows corte la conexión y el cliente pierda la respuesta.
        while size > 0:
            chunk = self.rfile.read(min(size, 65536))
            if not chunk:
                return
            size -= len(chunk)

    def _reply(self, status: int, payload: dict) -> None:
        data = json.dumps(payload, ensure_ascii=False).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        if status >= 400:
            # Con un error el cuerpo puede quedar sin leer: no reutilizar la conexión.
            self.close_connection = True
        elapsed = (time.perf_counter() - self._t0) * 1000
        # Antes de enviar: el cliente puede seguir en cuanto llega la respuesta, y el log debe existir ya.
        log.info("%s %s %d %.1f ms", self.command, urlsplit(self.path).path, status, elapsed)
        self.end_headers()
        self.wfile.write(data)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description="Sidecar de triaje de correo (Laya), solo en loopback.")
    parser.add_argument("--host", default=HOST, choices=[HOST], help="solo 127.0.0.1")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument("--checkpoint", choices=CHECKPOINTS, default=DEFAULT_CHECKPOINT,
                        help="checkpoint de Laya: base, typed-decisions, multilingual o router")
    parser.add_argument("--no-warmup", action="store_true", help="no hace la inferencia de calentamiento al arrancar")
    args = parser.parse_args(argv)
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(message)s", stream=sys.stderr)
    engine = Engine.load(args.checkpoint)
    if not args.no_warmup:
        engine.warm_up()
    server = TriageServer(engine, args.port)
    log.info("escuchando en %s:%d (checkpoint=%s, device=%s, loaded=%s)",
             HOST, args.port, engine.checkpoint, engine.device, engine.loaded)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
