# Contrato del puntuador de riesgo (CORE9-F)

Nivel 1.5 de [`../rfc/approvals-escalation.md`](../rfc/approvals-escalation.md)
§2.5. Se consulta solo cuando la política escala una acción a `lead` por
`no_rule` o `high_risk`, y nunca para `never_delegate`, `unknown_action` o una
acción sin categoría. Con `backend = "none"` (el defecto) no se consulta nada y
no se audita nada.

## Configuración: `[scorer]` en `approvals.toml`

```toml
version = 1

[scorer]
backend = "local_http"                   # none | local_http | cloud
endpoint = "http://127.0.0.1:8765"       # local_http: solo loopback, sin ruta
model = "clef-flash"                     # por defecto clef-flash
timeout_ms = 200                         # 1..200; el tope es el presupuesto fail-closed
auto_ok_if_low = ["read", "test", "build"]
low_below = 0.2                          # score < low_below → low
high_from = 0.7                          # score >= high_from → high; medio entre medias
```

- Se lee con el resto del fichero y se recarga por mtime. Un valor malo rechaza
  el fichero entero y se conserva la última política buena.
- `deny_unknown_fields`: una clave mal escrita en `[scorer]` rechaza el fichero.
- `auto_ok_if_low` no admite `never_delegate` (`money`, `publish`, `credentials`,
  `delete_user_data`, `outside_workspace`).
- `endpoint` de `local_http` es la base; el backend añade `/v1/chat/completions`.
  El RFC proponía `/score`; se usa la interfaz OpenAI-compatible de las
  decisiones del propietario del producto.
- `cloud` exige `https://` y `api_key_env`, que es el **nombre** de una variable
  de entorno, nunca la clave. En esta build no hay cliente TLS: el backend existe
  en el esquema y siempre falla cerrado.
- Ni la URL ni la clave aparecen en mensajes de error ni en logs.

## Protocolo `local_http`

`POST {endpoint}/v1/chat/completions` con `Connection: close`:

```json
{"model": "clef-flash", "temperature": 0, "max_tokens": 32,
 "messages": [{"role": "system", "content": "…"},
              {"role": "user", "content": "{\"category\":\"test\",\"tool\":\"bash\",\"cwd\":\"…\",\"command\":\"cargo test\",\"paths\":[]}"}]}
```

Respuesta válida: HTTP 2xx y `choices[0].message.content` con un objeto
`{"risk_score": x}` o el número `x`, con `0 ≤ x ≤ 1`. Cualquier otra cosa es un
fallo. Se acepta `Transfer-Encoding: chunked`. La respuesta está acotada a 64 KiB.

Solo viaja el **comando canónico** y las rutas redactadas; nunca el texto libre
del agente. El cliente es síncrono y tiene un único plazo para todo el
intercambio (conexión, envío y lectura).

## Veredicto y etiqueta

| Respuesta | Etiqueta | Puede aprobar |
|---|---|---|
| `risk_score < low_below` | `low` | sí, si la categoría está en `auto_ok_if_low` y el riesgo del host no es `high` |
| `low_below ≤ risk_score < high_from` | `medium` | no |
| `risk_score ≥ high_from` | `high` | no |
| fallo: timeout, conexión, HTTP no 2xx, respuesta no válida, backend `cloud`, respuesta tardía | — | no: escala al lead |

`high` nunca se aprueba por el puntuador. Una respuesta que llega después de
`timeout_ms` cuenta como fallo aunque sea válida.

## Escalada y auditoría

Transiciones (la escalada es monótona: `policy → scorer → lead → cto → user`):

- `scorer` aprueba: `status=approved`, `level=scorer`, `escalated_from=policy`,
  `decider_kind=scorer`, `decider_id=scorer`, `risk_score`, `scorer_version`.
- `scorer` no aprueba: `status=pending`, `level=lead`, `escalated_from=scorer`,
  `reason` según la tabla siguiente. El plazo de 40 s del lead corre desde ahí.

| `reason` | Cuándo |
|---|---|
| `scorer_unavailable` | fallo de la tabla anterior |
| `scorer_not_auto_ok` | score `low` pero categoría fuera de `auto_ok_if_low`, o acción de riesgo `high` |
| `scorer_risk` | score `medium` o `high` |

Convención de auditoría: `decider_id` es el actor fijo `scorer` (no el modelo);
la versión va en la columna `scorer_version` (`<modelo>@p1`). Cada consulta que
devuelve un veredicto deja una fila en `approval_audit` con
`decider_kind=scorer`, `decider_id=scorer`, `scorer_version` y `decision`:
`approved` si aprueba, `escalated` si sube al lead (no `pending`: la
aprobación sigue pendiente, pero la decisión del puntuador ya está tomada).
Cambiar la pregunta al modelo sube la revisión de prompt (`@p1` → `@p2`).

## Calibración

`crates/chat/tests/scorer_calibration.rs`: 60 acciones etiquetadas (34 seguras,
26 que no deben correr sin preguntar) ejecutadas por el camino real de
aprobación contra un servidor simulado en loopback. El servidor simulado es una
heurística por palabras clave, no un modelo: el resultado mide el camino y el
umbral, no Clef-flash. Las acciones y las reglas de `classify` se escribieron
juntas, así que estos números son **circulares**: no son una medida de acierto
fuera de la tabla.

Resultado con `low_below = 0.2` (recomendado), cada umbral 0.1–0.6 da lo mismo
porque el simulado solo devuelve tres valores:

| TP | FP | FN | TN | precisión | recall |
|---|---|---|---|---|---|
| 33 | 0 | 1 | 26 | 1,000 | 0,971 |

El único falso negativo es `findstr /s TODO *.rs`: `/s` empieza por `/` y
`names_outside` lo toma por ruta absoluta, así que la acción pasa a `command` y
no a `read`. Es un fallo conservador (sube al lead), no una aprobación indebida.
El umbral se mantiene en `0.2`; el recall de Clef-flash real es [NV].

## Tests de aceptación

`crates/chat/tests/risk_scorer.rs`: `backend=none` no cambia nada; aprobación
audita `scorer`; `low` fuera de `auto_ok` escala con su motivo; `medium` escala;
`high` nunca se aprueba por el puntuador; `never_delegate` no llega al
puntuador; conexión rechazada, respuesta no válida, HTTP 500 y `cloud` escalan
con `scorer_unavailable`; un servidor lento se corta en el presupuesto de 200 ms;
un fichero que no parsea no puntúa.
