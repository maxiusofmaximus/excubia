# CORE1 — Demostración manual (prototipo del núcleo)

Prueba manual real en esta máquina (Windows 11, ConPTY): un `host` propio
supervisa **dos agentes falsos** (`fake-agent.exe`, binario del workspace) y se
controla con `curl` contra la API local. El transcript crudo está en
[`evidence/core1-demo.txt`](evidence/core1-demo.txt); el flujo SSE capturado,
en [`evidence/core1-sse.txt`](evidence/core1-sse.txt); la salida del demonio, en
[`evidence/core1-host-out.txt`](evidence/core1-host-out.txt).

## Comando

```powershell
# 1) compilar (desde el worktree core\, con CARGO_TARGET_DIR=core\target)
cargo build -p hn-core-host --bins

# 2) arrancar el demonio con dos agentes falsos (fase "working" de 1,5 s)
target\debug\host.exe --port 8791 --label core1-demo `
  --spawn "target\debug\fake-agent.exe --delay-ms 1500" `
  --spawn "target\debug\fake-agent.exe --delay-ms 1500"

# 3) guion completo de la demo (reproducible)
powershell -NoProfile -ExecutionPolicy Bypass -File docs\core\evidence\core1-demo.ps1
```

## Qué se observó (resumen del transcript)

| Paso | Petición | Resultado |
|---|---|---|
| Estado inicial | `GET /v1/health` | `{"ok":true,"revision":6,"streams":1}` |
| Fase de trabajo | `GET /v1/state` | 2 agentes `working` (`counts:{"working":2}`) |
| Diálogo de aprobación | `GET /v1/state` | 2 agentes `blocked`, `attention:3` |
| Leer pantalla | `POST /v1/agents/w1:p1/read` | la caja `¿Quieres continuar?` con `Esc to cancel · Enter to confirm` |
| Enviar texto | `POST .../respond {"text":"y"}` | `{"ok":true,...}`; el agente imprime `RECIBIDO: y` y `terminado` |
| Fin de tarea | `GET /v1/state` | p1 `done` (atención 2), p2 sigue `blocked` |
| Enviar tecla | `POST .../keys {"keys":["enter"]}` | p2 también termina → `counts:{"done":2}` |
| Marcar visto | `POST .../seen` | p1 pasa a `idle` (se limpia la insignia `done`) |
| Flujo en vivo | `GET /v1/events` | primer frame `event: state` y cambios `pane.agent_status_changed` con `revision` 6→11 y `state_change_seq` global creciente |

Los frames SSE capturados incluyen, en orden:

```
event: state                      (revision 6, ambos working)
event: pane.agent_status_changed  (p1 blocked, seq 5)
event: pane.agent_status_changed  (p2 blocked, seq 6)
event: pane.agent_status_changed  (p1 done,    seq 7)
event: pane.agent_status_changed  (p2 done,    seq 8)
event: pane.agent_status_changed  (p1 idle tras /seen, mismo seq 7: no es una transición de estado del agente)
```

## Notas de la demostración

- La API escucha **solo en 127.0.0.1**; sin emparejado ni autenticación (fase
  siguiente, ver [NV] en `CORE1-RESULT.md`).
- ConPTY pide la posición del cursor (`ESC[6n`) al arrancar y espera respuesta;
  el `host` la contesta desde el emulador (`vt100`), igual que herdr la contesta
  desde ghostty-vt. Sin esa respuesta, el agente no emite salida.
- El guion guarda además `core1-host-out.txt` (stdout del demonio) y
  `core1-host-err.txt`.
