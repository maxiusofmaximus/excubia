# CORE3 — Demostración manual (crear agentes, hooks y restauración)

Prueba manual real: el demonio `host` con catálogo cerrado de agentes, un
cliente firmado (`demo-client`) que crea una conversación por API, un hook que
informa el estado del agente, la muerte del panel y la restauración tras
reiniciar el host. Transcript crudo en
[`evidence/core3-demo.txt`](evidence/core3-demo.txt); SSE en
[`evidence/core3-sse.txt`](evidence/core3-sse.txt).

## Comando

```powershell
cargo build -p hn-core-host -p hn-core-test-support --bins
powershell -NoProfile -ExecutionPolicy Bypass -File docs\core\evidence\core3-demo.ps1
```

## Qué se observó (resumen del transcript)

| Paso | Resultado |
|---|---|
| `pair-code` + pair + `devices grant respond` | dispositivo con `read_state` + `respond` |
| `create {agent:"fake", cwd, label:"demo chat"}` | `200`: `pane_id w1:p1`, `agent_status unknown`, título `demo chat` |
| `create` con agente desconocido / cwd inexistente | `422` con `loc:["agent"]` / `loc:["cwd"]` |
| el agente llega a su diálogo | `blocked` (patrón de pantalla) |
| hook: `POST /v1/hooks/report {state:"working"}` | `200`; el estado pasa a `working`: **el hook gana al patrón** |
| 6 s después (TTL 5 s) | vuelve a `blocked`: el patrón habla de nuevo |
| `respond "y"` → `respond "salir"` | `done` y, al morir el proceso, el panel desaparece |
| `respond` sobre el panel muerto | `400 {"code":"pane_gone","message":"pane w1:p1 is no longer known"}` |
| SSE | `pane.created`, `pane.updated`, cambios de estado con `revision`/`state_change_seq`, `pane.closed`, `: keepalive` |
| segundo panel vivo → matar el host → reiniciar | el panel vuelve con `restored:true`, `status unknown`, mismo título |
| `read` del panel restaurado | su último diálogo, tal cual quedó |
| `respond` sobre el restaurado | `409 {"detail":"the pane has no running agent"}` |

## Notas

- El panel que murió **antes** del reinicio sigue siendo `pane_gone`: solo se
  restauran los paneles vivos cuando el host se apagó (o se mató a lo bruto;
  el snapshot se escribe con debounce de 2 s).
- La identidad de prueba vive en `evidence\core3-run\` (artefacto desechable,
  gitignored); el host solo guarda claves públicas.
- El catálogo `agents.toml` del demo mapea `fake` al binario de pruebas; un
  cliente nunca nombra un ejecutable.
