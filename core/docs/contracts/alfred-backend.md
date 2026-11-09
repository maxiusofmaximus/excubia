# Contrato: motor de Alfred desde el móvil

Cambiar el motor de Alfred (Claude ↔ modelo local de Strata) desde la app, con un botón y una
confirmación del dueño. No hay cambio automático ni arranque por cuota. Detalle del modelo y de
Strata: `local-backend.md`.

| Método | Ruta | Permiso | Uso |
|---|---|---|---|
| `GET` | `/v1/alfred/backend` | `read_state` | Estado |
| `POST` | `/v1/alfred/backend` | `approve` | Cambiar: `{"backend": "local" \| "claude"}` |

Sin el permiso: `403` `{"detail": "forbidden"}`. El agente no tiene `approve`.

## Estado

```json
{ "backend": "claude", "state": "claude", "detail": null, "since": 1762200000, "voice_suspended": false, "engines": [] }
```

- `backend`: el motor que usan los turnos (`claude` o `local`, según `alfred-local`).
- `state`: `claude`, `starting`, `local`, `stopping` o `failed`.
- `detail`: texto en español cuando hace falta (fallo, o modelo en marcha sin usarse), si no `null`.
- `since`: unix; desde el último cambio que vio este proceso (al arrancar, el arranque).
- `voice_suspended`: `true` mientras `local-mode` existe o un arranque local está en curso.
- `engines`: motores CLI `Ready` en este PC, como `[{"name": "opencode", "version": "2.2.6"}]`.
  Vacío (`[]`) si ninguno. Solo lectura: el cambio sigue siendo `claude` o `local`, y no se
  puede pedir un motor de esta lista. Un host anterior no envía la clave; el móvil la trata como vacía.

El estado sale de los marcadores (`alfred-local`, `local-mode`) y del estado real de Strata. Lo único
que guarda el proceso es la tarea en curso y el último fallo.

## Cambiar

- `local`: ya `local` → `200`. Si no → `202` con `state: "starting"`. La tarea hace lo mismo que
  `host alfred local`: arranca Strata, espera a que responda y solo entonces escribe `alfred-local`.
- `claude`: ya `claude` con la voz libre → `200`. Si no → `202` con `state: "stopping"`. La tarea borra
  `alfred-local` y para Strata (libera la RAM y devuelve la voz).
- Pedir lo mismo que ya está en marcha → `202` (una sola tarea). Pedir lo contrario mientras hay una
  tarea → `409` `{"detail": {"code": "backend_busy", "message": "..."}}`.
- Otro valor, o sin `backend` → `400` `{"detail": {"code": "invalid_backend", ...}}`.
- No bloquea: la respuesta llega antes de que Strata cargue (tarda unos 4 min).

## Fallo

Sin binario de Strata, sin RAM, o tiempo agotado: `state: "failed"`, `detail` en español, `backend`
sigue en `claude`, y `alfred-local` no queda puesto. Un `POST` nuevo reintenta. Un fallo al parar deja
el motor local como estaba y lo dice en `detail`.

## Avisos y auditoría

- Avisos del sistema: «Alfred usa ahora el modelo local» y «Volvió a Claude», una vez por cambio (los
  pone el watch de `alfred-local`, también para cambios hechos por CLI); «No pude arrancar el modelo
  local: …» una vez por fallo.
- Auditoría: una línea en el log del daemon, `action alfred.backend device=<id> backend=<local|claude>
  outcome=<started|unchanged>`. No hay registro persistente de acciones del host.

## Catálogo de agentes

| Método | Ruta | Permiso | Uso |
|---|---|---|---|
| `GET` | `/v1/catalog` | `read_state` | Agentes que el host conoce y si Windows los arranca |
| `POST` | `/v1/catalog/{name}/fix` | `approve` | Apunta una entrada que Windows no arranca a su `.CMD` |

```json
{ "ok": true, "agents": [
  { "name": "codex", "available": true, "runnable": false, "fixable": true,
    "problem": "Windows no puede ejecutarlo tal como está instalado; se puede arreglar.", "label": "Codex" }
] }
```

- `available`: el ejecutable se encuentra (PATH y PATHEXT).
- `runnable`: Windows lo puede arrancar con la configuración actual. Un script sin extensión no.
- `fixable`: el host sabe apuntarlo a su shim `.CMD` (el `POST` exige `approve`).
- `problem`: causa en español, sin rutas ni comandos; `null` si no hay.
- Un host anterior no envía `runnable`: el móvil lo trata como `available`.

`POST /v1/catalog/{name}/fix` (sin cuerpo):

- `200` `{"ok": true, "fixed": true, "name": "codex"}` si cambió `agents.toml`; `fixed: false` si ya era ejecutable.
- `404` `unknown_agent`; `409` `not_fixable` (nada que arreglar aquí); `500` `write_failed` (no cambió nada).
- Antes de escribir copia `agents.toml` a `agents.toml.bak-<segundos>`, escribe un temporal y lo renombra.
  El catálogo se relee cuando cambia `agents.toml`.
- El nombre solo admite letras, dígitos, `-` y `_`; el móvil y el escritorio lo validan antes de enviarlo.

## No hace

Cambio automático, arranque por cuota agotada, cambio de modelo dentro de Strata, ni tarjeta de
aprobación en el móvil (la app es otra tarea).
