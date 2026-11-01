"""Acuerdo de Laya (modo sombra) con las decisiones reales del Master.

Cruza `mail-shadow.jsonl` con las decisiones de los lotes `mail.move` (`chat.db`) y con
`actions.jsonl`: borrar con OK = «eliminar»; dejar fuera, denegar o deshacer = «conservar».
Solo imprime agregados: ni ids, ni asuntos, ni remitentes. Es de solo lectura.

    python -I agreement.py <directorio de datos del host>
"""

import collections
import json
import sqlite3
import sys
from pathlib import Path

MIN_LABELS = 200  # RFC mail-decisions §6: antes de esto no se muestra nada.


def labels(data: Path) -> dict[str, str]:
    db = sqlite3.connect(f"file:{data / 'chat.db'}?mode=ro", uri=True)
    out: dict[str, str] = {}
    rows = db.execute(
        "SELECT status, payload_redacted FROM approval "
        "WHERE tool = 'mail.move' AND status IN ('consumed', 'denied') ORDER BY decided_at"
    )
    for status, payload in rows:
        for item in json.loads(payload or "{}").get("items", []):
            keep = status == "denied" or item.get("excluded")
            out[item["message_id"]] = "conservar" if keep else "eliminar"
    actions = data / "mail" / "accounts" / "actions.jsonl"
    if actions.exists():
        for line in actions.read_text(encoding="utf-8").splitlines():
            action = json.loads(line) if line.strip() else {}
            if action.get("tool") == "mail.move" and action.get("undone"):
                out[action["message_id_before"]] = "conservar"
    return out


def shadow(data: Path) -> dict[str, dict]:
    out: dict[str, dict] = {}
    for line in (data / "mail-shadow.jsonl").read_text(encoding="utf-8").splitlines():
        row = json.loads(line) if line.strip() else {}
        if row.get("status") == "ok":
            out[row["message_id"]] = row
    return out


def report(data: Path) -> None:
    given, seen = labels(data), shadow(data)
    pairs = [(given[m], seen[m]["sidecar"]["answers"]["action"]) for m in given if m in seen]
    print(f"etiquetados con ficha de sombra: {len(pairs)} (hacen falta {MIN_LABELS})")
    print("etiquetas:", dict(collections.Counter(y for y, _ in pairs)))
    table = collections.Counter((y, max(a, key=a.get)) for y, a in pairs)
    print("(etiqueta, opción más probable):", dict(table))
    for y in ("eliminar", "conservar"):
        scores = [a["eliminar"] + a["no_deseado"] for yy, a in pairs if yy == y]
        if scores:
            print(f"P(quitar) media si «{y}»: {sum(scores) / len(scores):.3f} (n={len(scores)})")


if __name__ == "__main__":
    report(Path(sys.argv[1]))
