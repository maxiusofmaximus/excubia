# Contrato: registro de acciones del host (`audit.jsonl`)

Un registro único de las acciones sensibles que el host ejecuta por petición. Lo lee el PC con
`host audit tail`. No lo ve el móvil y no se envía a ningún sitio.

## Dónde

`<data-dir>/audit.jsonl`, una línea JSON por evento, solo añadir (`O_APPEND`, un solo `write` por
línea que termina en `\n`).

Otros registros que siguen igual: `pairing-audit.jsonl` (decisiones de `POST /v1/pair`) y el
ledger de correo (`crates/mail/src/actions.rs`).

## Formato

```json
{"ts":1700000000,"device":"dev-…","action":"alfred.backend","detail":{"backend":"local"},"outcome":"started"}
```

| Campo | Valor |
|---|---|
| `ts` | Segundos Unix (entero). |
| `device` | Id del dispositivo que actúa o al que afecta la acción; `null` si no hay (p. ej., un rechazo). |
| `action` | Una de las acciones de la tabla siguiente. |
| `detail` | Objeto de cadenas. Cada valor se recorta a 200 caracteres. |
| `outcome` | Resultado, ver la tabla. |

Acciones registradas:

| `action` | Cuándo | `outcome` | `detail` |
|---|---|---|---|
| `alfred.backend` | `POST /v1/alfred/backend` | `started`, `unchanged`, `failed` (refusal `backend_busy`) | `backend` (`local`/`claude`); en `failed`, `error` |
| `pair` | Cada decisión de `POST /v1/pair` | `paired`, `recovered`, `rejected`, `pake_started`, `pake_failed` | `via` (`code`, `recovery`, `tailnet`, `admin`, `pake`) |
| `devices.grant` | `POST /v1/devices/{id}/grant` | `granted` | `by` (dispositivo que concede), `permissions` (separados por coma) |
| `devices.revoke` | `POST /v1/devices/{id}/revoke` | `revoked` | `by` (dispositivo que revoca) |

No se registra nada más. Una acción nueva se añade aquí y en la ruta que la ejecuta.

## Qué nunca se registra

Códigos de emparejamiento, códigos de recuperación, claves públicas o privadas, enlaces de
emparejamiento, cuerpos de mensajes ni cadenas de verificación (SAS). Los avisos al chat no se
duplican: el registro solo añade la línea.

## Rotación

Antes de añadir una línea que haría pasar el fichero de 5 MiB, `audit.jsonl` se renombra a
`audit.1.jsonl`, sustituyendo al anterior, y se empieza un fichero nuevo. Como mucho hay 10 MiB en
disco.

## Fallos

Escribir en el registro nunca falla la petición. Si no se puede escribir o rotar, el error va por
stderr y la acción sigue su curso.

## Leer

```
host audit tail [--n 50] [--data-dir DIR]
```

Imprime los últimos N eventos legibles, del más antiguo al más nuevo: hora local, dispositivo
(acortado), acción y resultado. Lee primero `audit.1.jsonl` y luego `audit.jsonl`. Una línea que no
sea un evento (escritura cortada, edición a mano) se salta sin fallar.
