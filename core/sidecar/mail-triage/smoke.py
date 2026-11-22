"""Smoke del sidecar con 14 fichas SINTÉTICAS (dominios example.*, datos inventados) contra el servidor real.

Uso: uv run python smoke.py   (arranca server.py dos veces: frío y caliente; luego mide)
"""

import http.client
import json
import math
import subprocess
import sys
import time
from pathlib import Path

import psutil

from synthetic import mk

sys.stdout.reconfigure(encoding="utf-8")
ROOT = Path(__file__).resolve().parent
PORT = 8766


FICHAS = [
    ("phish_banco_es", mk("avisos@banco-ejemplo.example", "Acción urgente: su cuenta será bloqueada hoy",
        "Confirme sus datos en el enlace en menos de 24 horas o su cuenta quedará suspendida.",
        name="Banco Ejemplo", auth=("pass", "fail", "fail"), reply_to="cuentas@mail-externo.example",
        links=1, link_host_differs=True, level="sospechoso", signals=["dmarc_fail", "reply_to_differs"])),
    ("phish_banco_en", mk("security@bank-ejemplo.example", "Security alert: unusual sign-in",
        "Verify your identity now or your access will be disabled.", name="Bank Example Security",
        auth=("fail", "fail", "fail"), reply_to="help@freemail-ejemplo.example", links=1,
        link_host_differs=True, level="sospechoso", signals=["dmarc_fail", "reply_to_differs"])),
    ("factura_es", mk("factura@proveedor.example", "Factura nº 2026-0412 de tu suscripción",
        "Tu factura de octubre ya está disponible en tu área de cliente. Importe: 19,90 EUR.",
        name="Proveedor Ejemplo", links=1, contact=True)),
    ("boletin_es", mk("novedades@tienda.example", "Novedades de octubre: nuevas colecciones",
        "Descubre las colecciones de otoño y los artículos más vendidos del mes.",
        name="Tienda Ejemplo", links=3, unsub=True)),
    ("marketing_es", mk("ofertas@tiendaxl.example", "¡Solo hoy! 50% de descuento en todo",
        "Última oportunidad: no te quedes sin tu descuento. Aprovecha ya.", name="TiendaXL", links=2, unsub=True)),
    ("personal_es", mk("ana.perez@correo.example", "¿Comida el sábado?",
        "¿Te apetece que quedemos a comer el sábado en el centro? Dime algo.", name="Ana Pérez", contact=True)),
    ("paquete_falso_es", mk("envios@correos-seguimiento.example", "Tu paquete no pudo entregarse: confirma la dirección",
        "Pulsa aquí y paga 1,99 EUR de gestión para recibir tu envío.", name="Correos Seguimiento",
        auth=("fail", "fail", "fail"), links=1, link_host_differs=True, level="sospechoso",
        signals=["dmarc_fail", "enlace_otro_host"])),
    ("sorteo_falso_en", mk("winner@prize-center.example", "Congratulations! You have won a gift card",
        "Claim your reward within 12 hours by entering your bank details.", name="Prize Center",
        auth=("fail", "none", "fail"), links=2, level="sospechoso", signals=["dmarc_fail"])),
    ("seguridad_es", mk("no-reply@cuenta.example", "Nuevo inicio de sesión en tu cuenta",
        "Se ha iniciado sesión desde un dispositivo nuevo. Si fuiste tú, no tienes que hacer nada.",
        name="Cuenta Ejemplo", contact=True)),
    ("seguridad_en", mk("no-reply@accounts.example", "New sign-in to your account",
        "A new device signed in. If this was you, no action is needed.", name="Accounts Example", contact=True)),
    ("newsletter_en", mk("news@devtips.example", "Weekly digest: 5 articles on Python",
        "This week: packaging tips, async patterns and a new release of the Python runtime.",
        name="DevTips", links=4, unsub=True)),
    ("personal_en", mk("tom.r@mail.example", "Photos from the trip",
        "Here are the photos from Lisbon. See you at dinner on Friday.", name="Tom", contact=True)),
    ("marketing_en", mk("deals@shop.example", "Your exclusive invite: 3 days only",
        "Members-only sale. Shop now before it ends on Sunday.", name="Shop Example", links=2, unsub=True)),
    ("factura_en", mk("billing@saas.example", "Your invoice for October is ready",
        "Your invoice for October 2026 is available in your account. Amount: 29.00 USD.",
        name="SaaS Example", links=1, contact=True)),
]


def gpu_used_mib() -> int:
    out = subprocess.run(["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"],
                         capture_output=True, text=True, check=True).stdout
    return int(out.splitlines()[0])


def gpu_process_mib(pid: int) -> str:
    out = subprocess.run(["nvidia-smi", "--query-compute-apps=pid,used_memory", "--format=csv,noheader,nounits"],
                         capture_output=True, text=True, check=True).stdout
    for line in out.splitlines():
        pid_str, _, mem = line.partition(",")
        if pid_str.strip() == str(pid):
            return f"{mem.strip()} MiB"
    return "N/A (nvidia-smi no lo lista por PID; en WDDM suele ocurrir)"


def request(method: str, path: str, body: dict | None = None) -> dict:
    conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=600)
    data = json.dumps(body).encode() if body is not None else None
    headers = {"Content-Type": "application/json"} if data else {}
    conn.request(method, path, body=data, headers=headers)
    resp = conn.getresponse()
    payload = json.loads(resp.read())
    conn.close()
    if resp.status != 200:
        raise RuntimeError(f"{method} {path} -> {resp.status} {payload}")
    return payload


def start_server() -> tuple[subprocess.Popen, float, dict]:
    t0 = time.perf_counter()
    proc = subprocess.Popen([sys.executable, str(ROOT / "server.py"), "--port", str(PORT)])
    while True:
        if proc.poll() is not None:
            raise RuntimeError(f"el servidor salió con código {proc.returncode}")
        try:
            health = request("GET", "/health")
        except OSError:
            time.sleep(0.5)
            continue
        if health["loaded"]:
            return proc, time.perf_counter() - t0, health
        proc.terminate()
        raise RuntimeError(f"el modelo no se cargó: {health}")


def process_tree(proc: subprocess.Popen) -> list[psutil.Process]:
    # En Windows el python del venv es un lanzador que arranca el intérprete real como hijo.
    root = psutil.Process(proc.pid)
    return [root, *root.children(recursive=True)]


def stop_server(proc: subprocess.Popen) -> None:
    procs = process_tree(proc)
    for p in procs:
        p.terminate()
    psutil.wait_procs(procs, timeout=60)


def top(probs: dict) -> tuple[str, float]:
    key = max(probs, key=probs.get)
    return key, probs[key]


def print_table(results: list[tuple[str, dict]]) -> None:
    print(f"{'ficha':<18} {'kind (top)':<30} {'P(phish)':>9} {'action (top)':<24} {'lat ms':>7}")
    for fid, resp in results:
        kind, kind_p = top(resp["answers"]["kind"])
        action, action_p = top(resp["answers"]["action"])
        print(f"{fid:<18} {kind + ' ' + format(kind_p, '.3f'):<30} "
              f"{resp['answers']['phish']['sí']:>9.3f} {action + ' ' + format(action_p, '.3f'):<24} "
              f"{resp['latency_ms']:>7}")


def nearest_rank(values: list[float], pct: float) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(pct * len(ordered)) - 1)]


def main() -> None:
    vram_before = gpu_used_mib()
    proc, cold_s, health = start_server()
    print(f"arranque 1 (frío): {cold_s:.1f} s · {health}")
    stop_server(proc)

    proc, warm_s, health = start_server()
    print(f"arranque 2 (caliente): {warm_s:.1f} s · {health}\n")
    try:
        first = request("POST", "/v1/triage", {"state": FICHAS[0][1]})
        results = [(fid, request("POST", "/v1/triage", {"state": f})) for fid, f in FICHAS]
        print_table(results)

        model_ms, wall_ms = [], []
        for _ in range(20):
            t0 = time.perf_counter()
            model_ms.append(request("POST", "/v1/triage", {"state": FICHAS[0][1]})["latency_ms"])
            wall_ms.append((time.perf_counter() - t0) * 1000)
        print(f"\n20 llamadas seguidas ({FICHAS[0][0]}): modelo mediana {sorted(model_ms)[10]} ms, "
              f"p95 {nearest_rank(model_ms, 0.95)} ms · ida y vuelta mediana {sorted(wall_ms)[10]:.1f} ms, "
              f"p95 {nearest_rank(wall_ms, 0.95):.1f} ms")
        print(f"primera inferencia tras arrancar: {first['latency_ms']} ms (modelo)")
        print(f"VRAM del proceso: {gpu_process_mib(proc.pid)}; GPU total usada antes de arrancar: "
              f"{vram_before} MiB, con el servidor cargado: {gpu_used_mib()} MiB")
        rss = sum(p.memory_info().rss for p in process_tree(proc))
        print(f"RAM del proceso servidor (RSS, árbol completo): {rss / 2**20:.0f} MiB")
    finally:
        stop_server(proc)


if __name__ == "__main__":
    main()
