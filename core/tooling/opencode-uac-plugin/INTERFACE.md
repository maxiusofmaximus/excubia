# Interfaz host ↔ plugin UAC (propuesta)

Estado: **propuesta para CORE8**. A 2026-10-21 el host no implementa estas rutas;
el plugin se prueba contra un servidor de eco local. No sustituye al contrato de
`docs/rfc/orchestrator-chat.md` §5: lo completa con la creación y el consumo.

## Invariantes

1. Solo loopback. El plugin rechaza `HN_HOOK_URL` cuyo host no sea `127.0.0.1`,
   `::1` o `localhost`, y rechaza URL con usuario/contraseña.
2. Token por run en `X-HN-Hook-Token` (el mismo `HN_HOOK_TOKEN` de `HOOKS.md`).
   El token nunca va en argv, prompt, URL ni logs.
3. TTL por defecto 120 s, máximo 300 s. El host es la autoridad de `expires_at`.
4. Un solo uso: tras `approved`, el plugin llama a `consume`; si falla, deniega.
5. Fail closed: timeout, HTTP no esperado, JSON inválido o estado desconocido
   bloquean la herramienta.

## Digest canónico

`action_digest = "sha256:" + SHA-256(JSON estable de {kind, tool, args, cwd, paths})`

- JSON estable: claves de objetos ordenadas; arrays conservan su orden; `paths`
  se ordena antes de serializar.
- `kind` ∈ `command | network | edit | tool`.
- El host debe recalcular el digest con el payload canónico del run o guardarlo
  como opaco y compararlo al decidir; no debe confiar en un digest de otro run.
- `payload_redacted` es solo para mostrar; puede no bastar para recalcular.

## `POST /v1/chat/approvals`

Crea una aprobación `pending`.

```json
{
  "client_request_id": "4d1c…",
  "pane_id": "w1:p9",
  "agent_id": "ag_bob",
  "run_id": "run_…",
  "session_id": "ses_…",
  "tool": "bash",
  "kind": "command",
  "summary": "Ejecutar comando: cargo test",
  "cwd": "D:\\proyectos\\demo",
  "paths": [],
  "action_digest": "sha256:…",
  "payload_redacted": {"command": "cargo test"},
  "ttl_seconds": 120
}
```

Respuesta `201`:

```json
{"approval_id":"apr_01J…","status":"pending","expires_at":"2026-10-21T10:12:00Z"}
```

Errores: `400 invalid_body`, `401 forbidden`, `403 forbidden` (token de otro run),
`409 approval_already_decided` si `client_request_id` ya existe con otra decisión,
`422 invalid_body` con `loc`.

## `GET /v1/chat/approvals/{id}`

Para sondeo del plugin. Respuesta `200`:

```json
{"approval_id":"apr_01J…","status":"pending","expires_at":"2026-10-21T10:12:00Z"}
```

`status` ∈ `pending | approved | denied | expired | consumed`. Un id de otra
empresa/run es `404 no_such_approval`. El host puede omitir `payload` aquí.

## `POST /v1/chat/approvals/{id}/consume`

Confirma el uso justo antes de ejecutar. Sin cuerpo o `{}`. Respuesta `200`:

```json
{"approval_id":"apr_01J…","status":"consumed"}
```

Errores: `409 approval_expired`, `409 approval_already_decided`,
`409 approval_mismatch` (cambió run/panel/digest), `404 no_such_approval`.

## Relación con el RFC

- `GET /v1/chat/approvals?status=pending` y
  `POST /v1/chat/approvals/{id}/decision` ya están en §3; no cambian.
- `POST /v1/chat/approvals`, `GET /{id}` y `POST /{id}/consume` son aditivos.
- La decisión del dispositivo sigue siendo `APPROVE` + `client_id`; el plugin
  nunca decide, solo espera y consume.
- El host debe emitir `chat.approval` y mantener el pane `blocked` mientras
  `pending`, como describe §5.2.

## Ejemplo de prueba

```sh
curl -s -X POST http://127.0.0.1:8787/v1/chat/approvals \
  -H "X-HN-Hook-Token: $HN_HOOK_TOKEN" -H "Content-Type: application/json" \
  -d '{"tool":"bash","kind":"command","summary":"echo hola","cwd":".","action_digest":"sha256:…","ttl_seconds":120}'
```
