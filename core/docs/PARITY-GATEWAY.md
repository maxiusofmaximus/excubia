# Paridad funcional: gateway Python → core Rust

Auditoría de solo lectura. El gateway (`gateway/`) queda obsoleto y no se modifica.
Este documento no cambia código: lista qué hace el gateway y qué hace el core.

Leyenda:

- ✔ implementado en el core con el mismo contrato.
- ✖ falta en el core.
- ➖ no aplica al core, por diseño (lo sustituye otra pieza o se descarta).
- `[NV]` no verificado: falta una prueba en dispositivo real o una decisión.

Prioridad (P1 bloquea la migración de usuarios, P2 mejora, P3 herramienta de operación):

## Rutas HTTP

| Gateway (`gateway/src/herdr_notify/api/`) | Core | Estado | Prio | Nota |
|---|---|---|---|---|
| health | `GET /v1/health` | ✔ | — | |
| pair: código, recuperación, dueño tailnet | `POST /v1/pair` | ✔ | — | La recuperación re-claves al dispositivo y rota el código. Dueño tailnet aprobado automáticamente (`tailscale whois`). |
| state | `GET /v1/state` | ✔ | — | |
| events | `GET /v1/events` | ✔ | — | |
| respond | `POST /v1/agents/{pane_id}/respond` | ✔ | — | |
| keys | `POST /v1/agents/{pane_id}/keys` | ✔ | — | |
| seen | `POST /v1/agents/{pane_id}/seen` | ✔ | — | |
| read | `POST /v1/agents/{pane_id}/read` | ✔ | — | |
| devices (lista) | `GET /v1/devices` | ✔ | — | Requiere `ADMIN`. |
| revoke | `POST /v1/devices/{id}/revoke` | ✔ | — | Revocar marca, no borra. |
| workspaces | `POST /v1/workspaces` | ✔ parcial | P2 | Solo crear. La semántica del gateway [NV]. |
| peers (lista de pares de la tailnet) | — | ✖ | P2 | Sin ruta en `crates/`. Depende de la decisión de transporte: si la lista de pares se descarta, pasa a ➖ [NV]. |
| focus | — | ✖ | P2 | Significado sin herdr [NV]. |
| image | — | ✖ | P2 | Significado sin herdr [NV]. |
| fcm-token | — | ✖ | P1 | Ligado a push FCM. Ver *Notificaciones*. |

Solo en el core (sin equivalente en el gateway): `POST /v1/chat/*` (conversaciones,
mensajes, delegaciones, aprobaciones), `GET /v1/catalog`, `POST /v1/agents/{pane_id}/stop`,
`POST /v1/hooks/report`. Las aprobaciones tipo UAC (`POST /v1/chat/approvals/{id}/decision`)
son la pieza que el gateway no tenía.

## Notificaciones y push

| Gateway | Core | Estado | Prio | Nota |
|---|---|---|---|---|
| `notify/policy.py` (cuándo notificar, cooldown) | — | ✖ | P1 | Decisión abierta 1: ¿el core necesita push? Antes de implementar. |
| `notify/fcm.py` (envío a FCM) | — | ✖ | P1 | Mismo bloqueo. La ruta M3 de FCM no está validada en dispositivo físico `[NV]`. |
| `notify/dpapi.py` (credenciales cifradas con DPAPI) | — | ✖ | P1 | Solo tiene sentido si hay FCM. |

## Atención y estado

| Gateway | Core | Estado | Prio | Nota |
|---|---|---|---|---|
| `notify/attention.py` (rango y conteos) | `crates/state/src/payload.rs` | ✔ | — | |
| `state/store.py` (estado persistido) | `crates/persist` + almacén de confianza (`crates/auth/src/store/`) | ✔ | — | Formato distinto; el importador traduce solo la confianza. |
| `watcher.py`, `herdr/*` (cliente de herdr) | sesiones propias y detección en `crates/host` | ➖ | — | herdr se sustituye por un host pty propio (decisión «Sin herdr, núcleo propio»). |

## Red

| Gateway | Core | Estado | Prio | Nota |
|---|---|---|---|---|
| `tailnet.py` (dueño de la tailnet) | `crates/auth/src/tailnet.rs` | ✔ parcial | P2 | Owner y aprobación ✔. Lista de pares ✖ (ver *Rutas HTTP*). |
| Escucha LAN con TLS y enlace fijado | `tls prepare` / `activate` | ✔ | — | Solo en core (CORE11). Puede no existir en el gateway. |

## CLI (`gateway/src/herdr_notify/cli.py`)

| Comando del gateway | Core | Estado | Prio | Nota |
|---|---|---|---|---|
| `enroll` | `host pair-code` | ✔ | — | Código de un solo uso, 600 s. |
| `devices` | `host devices` | ✔ parcial | — | Lista con REVOKED. |
| `devices --revoke <id>` | `host devices --revoke <id>` | ✔ | — | |
| `revoke-permission` | — | ✖ | P3 | El modelo de permisos del core no tiene revocar permiso. Ver `[NV]`: el gateway puede dejar un conjunto vacío explícito. |
| `add-device` | — | ✖ | P3 | La API de la tienda existe (`add_device`); falta el comando. |
| `dump` | — | ✖ | P3 | Volcado de herdr; equivalente con el host propio sin definir. |
| `status` | — | ✖ | P3 | |
| `push-credentials` | — | ✖ | P1 | Ligado a FCM. |
| `doctor` | — | ➖ | — | Diagnóstico de la conexión con herdr. |
| `schema` | — | ➖ | — | Depende de herdr. |
| `serve` | `host serve` | ✔ | — | |
| — | `host devices import-gateway` | ✔ nuevo | — | Solo en core. Ver `docs/contracts/pairing.md#migrar-desde-el-gateway-python`. |

## Formato de confianza (`devices.json`)

| Campo | Gateway | Core | Estado |
|---|---|---|---|
| `device_id`, `public_key`, `curve`, `label`, `revoked`, `permissions`, `owner_id` | ✔ | ✔ | Iguales; el importador los conserva. |
| `recovery_code_hash` (SHA-256 del código canónico) | ✔ | ✔ | Iguales; probado por test cruzado. |
| `read_state` al añadir un dispositivo | siempre | pendiente sin permisos (CORE11, D3) | Diferencia conocida. El importador conserva lo que había. |
| `fcm_token` | ✔ | — | No se importa. |
| Códigos de emparejamiento (hash) | ✔ en el mismo fichero | — | No se importan. |
| Curvas `p256` / `ed25519`, base64, tiempos en segundos | ✔ | ✔ | Iguales. |
| Permisos desconocidos | se descartan al cargar | se descartan con aviso al importar | Igual; el dispositivo se conserva. |

## Decisiones abiertas

1. ¿El core necesita push (`policy.py`, `fcm.py`, `dpapi.py`)? Bloquea P1.
2. Re-emparejo de la migración: el teléfono necesita el pin TLS de la LAN; hay que definir cómo llega sin un emparejamiento nuevo `[NV]`.
3. Significado de `focus` e `image` sin herdr.
4. ¿Se descarta la lista de pares (`peers`) bajo la independencia de transporte? RFC pendiente.
5. Dispositivos sin `recovery_code_hash` deben volver a emparejarse.
6. ¿Se quieren los comandos `revoke-permission`, `add-device`, `dump` y `status`? Son P3.
