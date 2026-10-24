# OpenCode UAC plugin

Plugin mínimo para que OpenCode pida aprobación al host antes de una acción de
riesgo y **deniegue por defecto** si no hay decisión. No usa dependencias: solo
`node:crypto`, `node:path` y `fetch`.

## Qué hace

- Shell (`bash`, `shell`, `powershell`, `cmd`, …): siempre pide UAC.
- Red (`webfetch`, `fetch`, `websearch`): siempre pide UAC.
- Edición/escritura (`edit`, `write`, `apply_patch`, …): pide UAC si la ruta
  queda fuera del `cwd`; `apply_patch`/`patch` siempre, porque la ruta puede
  estar enterrada en el parche.
- `HN_UAC_EXTRA_TOOLS`: lista separada por comas para sumar herramientas MCP u
  otras que el operador quiera aprobar.

El host debe implementar la interfaz de `INTERFACE.md`
(`POST /v1/chat/approvals`, `GET /{id}`, `POST /{id}/consume`). Mientras no
exista, el plugin se prueba con un servidor de eco local.

## Instalación (OpenCode 2.x)

En el `opencode.json` del asiento aislado:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "plugins": ["D:\\Documentos\\Projects\\herdr-notify-wt\\core\\tooling\\opencode-uac-plugin"],
  "permissions": [{ "action": "*", "resource": "*", "effect": "allow" }]
}
```

`permissions` debe dejar pasar la herramienta para que el hook la vea; si el
permiso nativo la deniega o la pregunta antes, el plugin no se ejecuta. La
aprobación real la da este plugin, no el permiso nativo.

OpenCode V1 (verificado con 1.19.3) también puede cargarlo: el módulo exporta
`{ id, setup, server }`; V1 usa `server()` y su hook `tool.execute.before`.
La entrada V1 no necesita cambios: `server(input)` toma `input.directory` como
`cwd` y el hook recibe `(input, output)`, con los argumentos en `output.args`.

### OpenCode V1 (1.19.3)

En V1 la clave es `plugin` (singular, array de rutas), no `plugins`, y los
permisos son un objeto por herramienta:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "plugin": ["D:\\ruta\\absoluta\\a\\opencode-uac-plugin"],
  "permission": { "bash": "allow", "edit": "allow", "read": "allow", "webfetch": "allow" }
}
```

V1 solo bloquea si el hook lanza; el adaptador `server()` ya lo hace. Pero V1
**no falla cerrado si el plugin no carga**: registra `failed to load plugin` en
sus logs y sigue ejecutando las herramientas sin UAC. Por eso el host debe
verificar el arranque (preflight) y no confiar solo en que la ruta esté en la
configuración.


## Variables

| Variable | Obligatoria | Uso |
|---|---|---|
| `HN_HOOK_URL` | sí | Base loopback del host, p. ej. `http://127.0.0.1:8787` |
| `HN_HOOK_TOKEN` | sí | Token por run; se envía en `X-HN-Hook-Token` |
| `HN_PANE_ID` | recomendada | Panel que se marca `blocked` |
| `HN_RUN_ID` | opcional | Run de la delegación |
| `HN_UAC_TTL_SECONDS` | no | TTL, por defecto 120 s (máx. 300) |
| `HN_UAC_POLL_MS` | no | Sondeo, por defecto 750 ms |
| `HN_UAC_EXTRA_TOOLS` | no | Herramientas extra separadas por comas |

Sin `HN_HOOK_URL` o `HN_HOOK_TOKEN`, toda acción de riesgo se deniega. No hay
modo fail-open.

## Garantías y límites

- Fail closed: timeout, error HTTP, JSON inválido, estado desconocido o
  `consume` fallido bloquean la herramienta.
- Un solo uso: el plugin consume la aprobación antes de devolver el control.
- El digest se calcula sobre argumentos completos; el payload que se muestra va
  redactado (`token`, `secret`, `password`, `api_key`, `authorization`, …).
- No lee credenciales ni keystores; solo el token de hook del entorno.
- Límite: la comprobación de “fuera del cwd” es léxica (`path.resolve`), no
  resuelve symlinks/junctions. El host debe reforzar la política con su propia
  canonicalización.
- Límite: `apply_patch` se bloquea siempre; no se intenta parsear el parche.

## Pruebas

```sh
pnpm test
```

Las pruebas levantan un host de eco en `127.0.0.1` y cubren: aprobación,
denegación, timeout, host caído, consumo fallido, digest estable, redacción y
el cableado del adaptador V1 (`server()` → `output.args`/`directory`, bloqueo
y acciones dentro de la política).

## E2E con OpenCode V1

Requiere un binario V1 aislado. Ejemplo de instalación sin tocar la instalación
global (los scripts de `opencode-ai` no se ejecutan; se usa el binario de
`opencode-windows-x64` directamente):

```sh
pnpm add --dir target/opencode-v1 --ignore-scripts opencode-ai@1.19.3
```

El arnés levanta un proveedor OpenAI-compatible y un host de aprobaciones en
loopback (coste cero) y ejecuta los cinco casos:

```sh
cd tooling/opencode-uac-plugin
pnpm run e2e:v1
```

Cada caso aísla `HOME`, `USERPROFILE`, `XDG_*`, `OPENCODE_CONFIG` y
`OPENCODE_DISABLE_PROJECT_CONFIG`. Los logs quedan en
`target/opencode-v1/e2e/<caso>/run.log`. Variables: `OPENCODE_V1_BIN` (binario),
`E2E_TIMEOUT_MS`, `E2E_LOG_LEVEL`, `E2E_PURE`, `E2E_PLUGIN_DIR` y
`E2E_WORK_ROOT`.

Control negativo (plugin sin `server()`): V1 registra el error y **deja
ejecutar** la herramienta, sin pedir aprobación. Reproducirlo:

```sh
E2E_PLUGIN_DIR="$PWD/e2e/fixtures/plugin-sin-server" \
E2E_WORK_ROOT="$PWD/../../target/opencode-v1/e2e-negative" \
pnpm exec node e2e/run-v1-cases.mjs aprobada
```

El runner marcará en rojo las comprobaciones de aprobación: es el resultado
esperado de este control, no un fallo del plugin.

## Canario del host sin red

El preflight aísla OpenCode en `<data>/uac-canary`: `XDG_CONFIG_HOME` apunta a
`<data>/uac-canary/xdg/config`, así que el CLI trabaja en
`<data>/uac-canary/xdg/config/opencode`. La clave `"npm"` del proveedor es un
selector: en 1.19.3 los SDK `@ai-sdk/openai`, `@ai-sdk/openai-compatible` y
`@ai-sdk/anthropic` van compilados en el binario y **no** se instalan. Lo que sí
instala el CLI, con su npm embebido, es su propio SDK de plugins
(`@opencode-ai/plugin`, con `package-lock.json`) al cargar un plugin.

Para que la primera pasada del canario no necesite red, preinstala ese árbol con
pnpm en la caché del host (la versión debe coincidir con la del CLI):

```sh
pnpm add --dir "$APPDATA/herdr-notify/uac-canary/xdg/config/opencode" \
  --ignore-scripts @opencode-ai/plugin@1.19.3 @ai-sdk/openai-compatible@3.0.75
```

Comprobado con el binario 1.19.3 y `HTTP(S)_PROXY` muerto, `npm_config_offline`
y caché npm vacía: los dos casos reales pasan en 2,6 s y el positivo da `Safe`
(antes, en frío, ~7 s por caso y con red). El script
`tooling/seed-uac-canary.ps1` hace la preinstalación.

## Investigación previa (2026-10-21)

- API V2 de OpenCode 2.0.30: `@opencode/plugin@2.0.30` (MIT) expone
  `ctx.tool.hook("execute.before", …)`; lanzar dentro del hook rechaza la
  llamada **antes** de ejecutar la herramienta. Fuente:
  <https://opencode.ai/v2/docs/build/plugins/migrate-v1/> y tipos
  `dist/promise/tool.d.ts` del paquete.
- Plugins existentes revisados: `@agentapprove/opencode` 0.1.24 (licencia
  propietaria, servicio iPhone, no host local), `opencode-smart-approval` 0.7.1
  (MIT, revisor LLM), `opencode-approval-guard` 1.1.1 (MIT, revisor LLM),
  `opencode-permission-reviewer` 2.4.3 (Apache-2.0, revisor LLM) y
  `opencode-reviewer` 2.3.0 (MIT, revisor LLM). Ninguno espera una decisión
  externa del host con TTL, single-use y `consume`; por eso se escribe este.
