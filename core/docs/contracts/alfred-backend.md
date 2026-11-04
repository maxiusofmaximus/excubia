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
{ "backend": "claude", "state": "claude", "detail": null, "since": 1762200000, "voice_suspended": false }
```

- `backend`: el motor que usan los turnos (`claude` o `local`, según `alfred-local`).
- `state`: `claude`, `starting`, `local`, `stopping` o `failed`.
- `detail`: texto en español cuando hace falta (fallo, o modelo en marcha sin usarse), si no `null`.
- `since`: unix; desde el último cambio que vio este proceso (al arrancar, el arranque).
- `voice_suspended`: `true` mientras `local-mode` existe o un arranque local está en curso.

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

## No hace

Cambio automático, arranque por cuota agotada, cambio de modelo dentro de Strata, ni tarjeta de
aprobación en el móvil (la app es otra tarea).
