# Contrato: backend local bajo demanda (Strata, fases 1-2)

Estado: fase 1 implementada 2026-11-02; fase 2 (la voz cede recursos) implementada
2026-11-02. Sin Strata real verificado (lo lanza el Master).
Alcance: arrancar, parar y consultar un servidor de modelo local en el PC del
host, y registrar un agente que lo usa. Sin enrutado ni puntuador (RFC §3-4).

## Comandos

| Comando | Qué hace |
|---|---|
| `host strata status` | `parado`, `cargando` o `listo (http://127.0.0.1:8080)`, y `modo local: sí/no`. Solo lee. |
| `host strata start` | Escribe `local-mode`, espera memoria libre, lanza Strata y espera a `loaded: true` (tope 600 s). `--voz-parada` se acepta y no hace nada. |
| `host strata stop` | `strata launch --stop` y borra `local-mode`. Si ya estaba parado, solo borra el marcador. |
| `host strata enable-agent [--data-dir DIR]` | Escribe `[agents.claude-local]` si no existe. |
| `host alfred use local\|claude [--data-dir DIR]` | Elige el modelo de Alfred (`alfred-local`). `local` exige Strata `listo`. |
| `host alfred local [--data-dir DIR]` | Arranca Strata (como `host strata start`) y, solo si queda `listo`, pone `alfred-local`. Si falla, no escribe nada y sale con error. |
| `host alfred status [--data-dir DIR]` | `Alfred: modelo local` o `Alfred: Claude`. Solo lee. |

El ejecutable sale de `STRATA_BIN`, o de
`%LOCALAPPDATA%\Strata\terminal\0.1.47\bin\strata.cmd`. Nunca de un cliente.

## Estados

- **parado**: `/health` no responde y `strata launch --status --output json`
  no lista nada en marcha.
- **cargando**: hay un lanzamiento, pero `/health` no da `loaded: true`
  (~236 s con el modelo en NVMe).
- **listo**: `GET /health` devuelve `loaded: true`.

`start` sondea con espera creciente (2 s, doblando, tope 15 s). Si el
lanzamiento falla antes de cargar, `start` falla con su código de salida.

## Modo local: el fichero `local-mode`

`<data-dir>/local-mode` (vacío; `paths::local_mode_path`). Existe = modo local activo.

- **Escribe:** `host strata start`, antes de comprobar la memoria. Lo borra
  `host strata stop` tras parar, y `start` si falla o expira.
- **Lee:** el daemon (`local_mode::watch`), cada 2 s, solo si hay configuración de
  voz. Al aparecer: `VoiceService::suspend()` suelta el ASR (su worker acaba; una
  nota en curso termina antes) y avisa en «Avisos del sistema»: «Modo local: voz
  desactivada mientras corre el modelo local». Al desaparecer: `resume()` recarga
  el ASR en segundo plano y avisa «Modo local terminado: voz disponible».
- Mientras está suspendido, las transcripciones responden `unavailable` y nada
  arranca el motor.
- `GET /v1/state` añade `"voice": "unavailable" | "suspended" | "ready"` (campo
  aditivo; `ready` quiere decir que el ASR arranca con la primera nota).

### Si el daemon muere con el fichero puesto

Al arrancar, el daemon revisa `local-mode` antes de precargar la voz:

- Strata no corre (`strata::is_running`): borra el fichero huérfano y avisa de
  la salida.
- Strata corre: el fichero se queda y la voz arranca suspendida.
- Si el daemon arranca mientras un `start` espera memoria, puede borrar el
  marcador antes de que Strata lance: repetir `start` lo deja bien.

## Modelo de Alfred: el fichero `alfred-local`

`<data-dir>/alfred-local` (vacío; `paths::alfred_local_path`). Existe = Alfred
contesta con el modelo local. Solo lo elige el dueño, a mano.

- **Escribe:** `host alfred local` o `host alfred use local`, solo si Strata está `listo`. Lo borran
  `host alfred use claude`, `host strata stop` y un `start` fallido.
- **Lee:** el seat, al empezar cada turno (`SeatConfig::turn_env`). Cada turno es
  un `claude -p` nuevo: el cambio vale desde el turno siguiente; un turno en curso
  no cambia. Con el fichero el CLI recibe el entorno de `claude-local`
  (`strata::local_env`) y no recibe `ANTHROPIC_API_KEY`. Sin él, no cambia nada.
- **Aviso:** el daemon lo mira cada 2 s y avisa en «Avisos del sistema» solo si
  cambia: «Alfred ahora responde con el modelo local (más lento y menos preciso)»
  o «Alfred vuelve a Claude».
- **Huérfano:** al arrancar el daemon, con Strata parado, el fichero se borra y
  se avisa «Alfred vuelve a Claude».
- **No hace:** arrancar Strata por su cuenta, cambiar de modelo a mitad de turno ni tocar
  la política de aprobaciones. Si Strata se cae con el fichero puesto, el turno falla con
  `process_exit`; el Master lo quita con `use claude`.

## Cuota agotada

- **Detecta:** un turno de Claude que acaba `failed` cuyo extracto de salida cumple
  `seat::quota::looks_like_quota_exhausted`: «usage limit», «rate limit», «limit reached»,
  «limit will reset», «credit balance is too low», «quota», «out of extra usage» y `429`
  como número. No cuentan `overloaded`/`529` ni `process_exit: 1` sin más.
- **Avisa:** el host publica en «Avisos del sistema» el aviso de `alfred_backend::QUOTA_NOTICE`
  (pasar a local con `host alfred local`). Máximo uno cada 30 min (en memoria). No avisa si
  `alfred-local` ya está puesto, ni si el turno iba con el modelo local.
- **No hace:** cambio automático de modelo, tarjeta de aprobación en el móvil ni volver solo a
  Claude (mejora futura). El turno sigue fallando igual.
- **Pasar a local:** `host alfred local` (tarda unos 4 min, apaga la voz, necesita ~34 GB de RAM
  libre). Para volver: `host alfred use claude`.

## Preflight de `start`

1. Escribe `local-mode` (la voz cede al momento).
2. Si Strata está parado: **memoria libre ≥ 38 GiB** (`GlobalMemoryStatusEx`).
   El modelo ocupa ~34 GB. Si no llega, espera hasta 60 s a que el host de voz
   la libere; si no llega, falla. Sin daemon no hay espera.

## Lanzamiento

`strata launch --model q2_0 --context 64k --vision off --parallel 1 --tune skip
--access local --port 8080 --no-api-key --no-tui --yes`, sin shell y con el
entorno en lista blanca. `STRATA_API_KEY` nunca entra (Strata respondería 401).

## Entrada `claude-local`

```toml
[agents.claude-local]
command = "claude"
args = []
label = "Claude Code (local Strata)"
env = { ANTHROPIC_BASE_URL = "http://127.0.0.1:8080", ANTHROPIC_MODEL = "qwen3.8-flash-next-q2_0", ANTHROPIC_AUTH_TOKEN = "strata-local", CLAUDE_CODE_MAX_CONTEXT_TOKENS = "65536", CLAUDE_CODE_MAX_OUTPUT_TOKENS = "4096" }
```

Reglas de `env` (`crates/api/src/catalog.rs`): nombre `^[A-Z][A-Z0-9_]*$`, valor
sin saltos de línea; nombres con `KEY`, `TOKEN`, `SECRET` o `PASSWORD` se rechazan,
salvo `ANTHROPIC_AUTH_TOKEN = "strata-local"` (placeholder que Strata documenta; el
servidor local no comprueba clave) y las cuentas `CLAUDE_CODE_MAX_*_TOKENS`
(sin ellas Claude Code no compacta). Un catálogo con una entrada inválida no se
carga.

## Límites

- **Una cosa a la vez.** Ocupa ~34 GB de RAM y 10,8 GB de VRAM. No convive con
  el host de voz (~4,7 GB de VRAM) ni con un navegador abierto. Solo bajo demanda.
- Solo `127.0.0.1`.
- ~100 tok/s con el modelo en NVMe; ~8 tok/s con el modelo en HDD. `--tune
  quick` no mejora (medido).
- Respuestas cortas pueden salir vacías si `max_tokens` es bajo (el modelo
  razona antes).
- Si `start` expira, borra `local-mode` aunque Strata siga cargando.

## Licencia

**[NV]** Pesos sin licencia verificada: solo uso local, sin redistribuir.
