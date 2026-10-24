# Hooks: how an agent reports its own state

Screen patterns are a fallback. Every agent the host starts gets what its
hooks need: `HN_ENV=1`, `HN_PANE_ID`, `HN_HOOK_TOKEN` (`host hook-token`),
`HN_HOOK_URL`. A hook POSTs `{"pane_id", "state":
"idle|working|blocked|unknown", "message"?}` to `$HN_HOOK_URL/v1/hooks/report`
with `X-HN-Hook-Token`. Only loopback is accepted, so the configuration holds
no secret; a fresh report wins over the screen for 45 s (`--hook-ttl`), then
the patterns speak again. `message` names a pane that has no name yet.

## Claude Code — `~/.claude/settings.json`

```json
{"hooks": {
  "PreToolUse":   [{"matcher": "*", "hooks": [{"type": "command", "command": "hn-report working"}]}],
  "Notification": [{"hooks": [{"type": "command", "command": "hn-report blocked"}]}],
  "Stop":         [{"hooks": [{"type": "command", "command": "hn-report idle"}]}]}}
```

`hn-report` is a tiny script beside your other tools:

```sh
#!/bin/sh
[ -n "${HN_PANE_ID:-}" ] && [ -n "${HN_HOOK_TOKEN:-}" ] || exit 0
curl -s -m 2 -X POST "${HN_HOOK_URL}/v1/hooks/report" -H "X-HN-Hook-Token: $HN_HOOK_TOKEN" \
  -H "Content-Type: application/json" -d "{\"pane_id\":\"$HN_PANE_ID\",\"state\":\"$1\"}" >/dev/null || true
```

## opencode — `~/.config/opencode/plugin/hn-state.js`

```js
const report = (state) => fetch(`${process.env.HN_HOOK_URL}/v1/hooks/report`, { method: "POST",
  headers: { "content-type": "application/json", "x-hn-hook-token": process.env.HN_HOOK_TOKEN },
  body: JSON.stringify({ pane_id: process.env.HN_PANE_ID, state }) }).catch(() => {});
const STATES = { "session.idle": "idle", "permission.asked": "blocked", "tool.execute.before": "working" };
export const HnStatePlugin = async () => ({
  event: async ({ event }) => { if (STATES[event?.type]) await report(STATES[event.type]); },
});
export default { id: "hn.state", server: HnStatePlugin };
```

## El hook bloqueante de las aprobaciones (CORE8)

`hn-uac-hook` es el hook que se ejecuta **antes** de una acción de riesgo. Abre
una aprobación en el host (`POST /v1/chat/approvals` con `X-HN-Hook-Token`),
espera la decisión del operador y responde con el código de salida que el CLI
entiende:

| Código | Significado |
|--------|-------------|
| `0` | la acción puede ejecutarse: el operador la permitió y la aprobación se consumió |
| `2` | la acción está denegada: el operador la rechazó, la aprobación expiró, el host no respondió o el binario recibió un argumento inválido |

El hook lee `HN_HOOK_URL` y `HN_HOOK_TOKEN` del entorno, nunca de los
argumentos, y el payload de la acción viaja como argumentos:

```sh
hn-uac-hook --kind command --tool bash --summary "Ejecutar comando: cargo test" \
  --args '{"command":"cargo test"}' --cwd /ruta/del/proyecto --ttl 120
```

Con Claude el payload llega por `stdin` (el formato `PreToolUse`), así que
bastan `--kind`, `--tool`, `--summary` y el resto en la línea de comandos.
El contrato completo está en [`../contracts/api.md`](../contracts/api.md) y el
código, en `crates/seat/src/bin/hn-uac-hook.rs`.

### Qué garantiza el asiento y qué no

Claude Code bloquea **solo** con el código `2`. Cualquier otro fallo del hook
lo trata como no bloqueante y ejecuta la acción. Por eso:

- **Panic:** `main` corre el hook dentro de `catch_unwind`; un panic sale con `2`
  (test `a_panic_is_a_denial_not_an_unblocking_exit`). Antes salía con `101`.
- **Binario ausente o no ejecutable:** `prepare` del asiento Claude comprueba
  `hook_ready` antes de escribir la configuración y rechaza el lanzamiento
  (`LaunchError::HookMissing`). El turno falla con `prepare_failed` y no se ejecuta
  nada. En Windows «ejecutable» es terminar en `.exe`; en Unix, bit de ejecución.
- **Permisos del asiento:** `Edit/Write/MultiEdit/NotebookEdit/WebFetch/Bash` están
  en `permissions.allow` porque el hook es la única puerta.

**No garantizado** (documentado, no resuelto aquí):

- **Aborts que no son panic:** OOM, desbordamiento de pila o `panic = "abort"` no
  pasan por `catch_unwind` y dan un código distinto de `2`, que Claude trataría
  como no bloqueante. El perfil de build no fija `panic = "abort"`; OOM y pila no
  están cubiertos.
- **Carrera entre comprobación y ejecución:** el binario puede borrarse o
  cambiarse entre `hook_ready` y el lanzamiento de Claude. El asiento solo
  comprueba al lanzar, no en cada llamada de herramienta.
- **Timeout del hook en Claude Code:** si Claude corta el hook por tiempo, el
  comportamiento depende de su versión. El asiento fija el timeout en TTL + 30 s
  para que el hook responda antes; no hay prueba de qué hace Claude al cortarlo
  [NV].
- **Muerte por señal externa** (kill, cierre de sesión): el código de salida no
  es `2`.
