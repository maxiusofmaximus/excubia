# CORE4 — Demostración manual (catálogo y parada de agentes)

Prueba manual real: el catálogo firmado, crear una conversación y pararla.
Transcript crudo en [`evidence/core4-demo.txt`](evidence/core4-demo.txt).

## Comando

```powershell
cargo build -p hn-core-host -p hn-core-test-support --bins
powershell -NoProfile -ExecutionPolicy Bypass -File docs\core\evidence\core4-demo.ps1
```

## Qué se observó (resumen del transcript)

| Paso | Resultado |
|---|---|
| `GET /v1/catalog` | `fake` `available:true` con `label:"Agente falso"`; `missing` `available:false`; sin comandos, args ni rutas |
| `create {agent:"fake", cwd, label:"para parar"}` | `200`; el panel queda `blocked` en `state` |
| procesos `fake-agent` | 1 antes de parar |
| `POST /v1/agents/w1:p1/stop` | `200 {"ok":true,"pane_id":"w1:p1"}` |
| procesos `fake-agent` tras parar | 0 (sin huérfanos) |
| `GET /v1/state` | lista vacía; el SSE publicó `pane.closed` |
| `respond` sobre el panel parado | `400 {"code":"pane_gone",…}` |
| segundo `stop` | `400 {"code":"pane_gone",…}` |

## Hint canónico de concesión (Desktop y APK lo muestran literal)

```
host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]
```

## Notas

- `available` dice si el comando resolvería en esta máquina; nunca revela el
  comando, sus argumentos ni una ruta.
- El catálogo se lee al arrancar el host: editar `agents.toml` exige reiniciar
  (no hay recarga en caliente todavía).
- La identidad de prueba vive en `evidence\core4-run\` (artefacto desechable,
  gitignored); el host solo guarda claves públicas.
