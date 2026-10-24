# CORE2 — Demostración manual (emparejado, auth y ciclo completo)

Prueba manual real en esta máquina: el demonio `host` (Rust) con dos agentes
falsos, un cliente firmado de prueba (`demo-client`) y **el cliente real de
`desktop/core`** contra el núcleo. El transcript crudo está en
[`evidence/core2-demo.txt`](evidence/core2-demo.txt); los frames SSE, en
[`evidence/core2-sse.txt`](evidence/core2-sse.txt); la salida del demonio, en
[`evidence/core2-host.txt`](evidence/core2-host.txt).

## Comando

```powershell
# 1) compilar (CARGO_BUILD_JOBS=4 y CARGO_TARGET_DIR=core\target)
cargo build -p hn-core-host -p hn-core-test-support --bins

# 2) guion completo (arranca el host, empareja, fluye, revoca y prueba el
#    cliente del desktop; deja la salida en evidence\)
powershell -NoProfile -ExecutionPolicy Bypass -File docs\core\evidence\core2-demo.ps1
```

## Qué se observó (resumen del transcript)

| Paso | Resultado |
|---|---|
| `host pair-code` | `RANDTIA`, válido 600 s, un solo uso; solo su hash queda en `devices.json` |
| `demo-client pair` | `HTTP 200`: `device_id`, `permissions:["read_state"]`, `recovery_code` (una sola vez), `owner_id:""` |
| `host devices list` | el dispositivo con `[read_state]` |
| `host devices grant -p respond/send_key` | ampliar permisos es un acto de host (CLI) |
| `demo-client state` | firmado; agentes `blocked`/`working`, `revision` 7 |
| `demo-client respond w1:p1 "y"` | `200`; `read` muestra `RECIBIDO: y`, `terminado` y el prompt |
| `demo-client keys w1:p2 enter` | `200`; ambos agentes quedan `done` |
| SSE | frame `state` inicial y cambios con `revision` 6→10 y `state_change_seq` 5→8 |
| `host devices revoke` (otro proceso) | el demonio lo detecta; el siguiente request es `401 {"detail":"unauthorized"}` |
| cliente de `desktop/core` | `pair OK`, `state OK`, `respond OK`, `read OK (eco recibido)`, `seen OK` |

## Notas

- El cliente de la demo guarda su clave en `evidence\core2-run\` (artefacto
  desechable, gitignored): es el «keystore» del teléfono de prueba. El núcleo
  nunca guarda claves privadas — `devices.json` solo tiene claves públicas.
- La herramienta `evidence\desktop-compat\` usa `desktop/core` como
  dependencia (fuera del workspace) para comprobar el cliente real; la app GUI
  no se lanzó porque el emparejado de la ventana exige clics manuales.
- Sin emparejado no hay nada: `/v1/health` y `POST /v1/pair` son las únicas
  rutas públicas; el resto responde `401` sin firma válida.
