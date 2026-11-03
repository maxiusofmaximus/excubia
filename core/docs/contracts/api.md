# Contract: HTTP surface
_(implementado en el núcleo Rust; ver README)_

The host binds loopback by default. Reaching it from a phone is the transport's
job, not this process's: over Tailscale, or over the LAN listener below. Authoritative
code: `crates/api/src/routes/`, `crates/api/src/lib.rs`.

## LAN TLS listener — IMPLEMENTED (CORE11)

`host --tls-bind <ip:port>` serves the **device routes** on a second listener:
TLS 1.3 only, the certificate pinned by the device (see the *LAN TLS pairing
link* in [`pairing.md`](pairing.md)). It is off by default, and a wildcard
address is refused. It mounts the same device routes as the loopback listener
and nothing else: `POST /v1/hooks/report` and the hook approval routes answer
`404` there, and the internal MCP surface is never served on it. A plaintext
request to the port is refused at the handshake.

## Rules

- The client names an **action**, never a herdr method. There is no passthrough.
- Every endpoint except `/v1/health` and `POST /v1/pair` requires a valid signature.
- **`POST /v1/pair` is the only unauthenticated write.** It is gated by a
  host-printed single-use code, a per-device recovery code, or the same-tailnet
  owner check — never a static secret (Fase 2(c), `pairing.md`).

## Endpoints

| Method | Path | Auth | Grant | Rate | Purpose |
|--------|------|------|-------|------|---------|
| `GET` | `/v1/health` | none | — | — | Liveness, no operator data |
| `POST` | `/v1/pair` | none (code/owner) | — | write (per IP) | Pair or re-key a device |
| `POST` | `/v1/pair/pake/start` | none (PAKE code, TLS only) | — | write (`pake:<ip>`) | PAKE pairing, step 1 (F2b, alpha) |
| `POST` | `/v1/pair/pake/finish` | none (PAKE code, TLS only) | — | write (`pake:<ip>`) | PAKE pairing, step 2: confirm and register a pending device |
| `GET` | `/v1/state` | signed | `READ_STATE` | read | The full state read model |
| `GET` | `/v1/events` | signed | `READ_STATE` | read | SSE stream (see `state.md`) |
| `GET` | `/v1/catalog` | signed | `READ_STATE` | read | The agents this host can start, by name |
| `GET` | `/v1/roles` | signed | `READ_STATE` | read | The role catalogue: areas, roles and levels (not the CLIs of `/v1/catalog`) |
| `GET` | `/v1/hires` | signed | `READ_STATE` | read | The professionals contracted (`?include_removed=true` adds the dismissed) |
| `POST` | `/v1/hires` | signed | `APPROVE` | write | Contract one role: `201` when new, `200` when already hired |
| `POST` | `/v1/hires/{hire_id}/update` | signed | `APPROVE` | write | Rename a hire or change its avatar |
| `POST` | `/v1/hires/{hire_id}/dismiss` | signed | `APPROVE` | write | Dismiss a hire (idempotent) |
| `GET` | `/v1/onboarding` | signed | `READ_STATE` | read | Whether the first-run onboarding is done, and its areas |
| `GET` | `/v1/alerts/rules` | signed | `READ_STATE` | read | The alert rules (`{ok, rules}`) |
| `POST` | `/v1/alerts/rules` | signed | `RESPOND` | write | Create an alert rule: `201 {ok, rule}`, `409` when identical |
| `POST` | `/v1/alerts/rules/{rule_id}/update` | signed | `RESPOND` | write | Turn a rule on or off: body `{"enabled": bool}` |
| `POST` | `/v1/alerts/rules/{rule_id}/delete` | signed | `RESPOND` | write | Delete a rule (reversible: create it again) |
| `GET` | `/v1/alerts/mode` | signed | `READ_STATE` | read | The alert mode: `all` or `rules_only` |
| `POST` | `/v1/alerts/mode` | signed | `RESPOND` | write | Set the alert mode: body `{"mode": "all" \| "rules_only"}` |
| `POST` | `/v1/onboarding` | signed | `APPROVE` | write | Finish the onboarding: contract the team of the chosen areas |
| `GET` | `/v1/peers` | signed | `READ_STATE` | read | Tailnet peers + host addresses to probe |
| `GET` | `/v1/devices` | signed | `ADMIN` | read | List paired devices ("My devices"); a pending device carries its `sas` |
| `POST` | `/v1/devices/{device_id}/revoke` | signed | `ADMIN` | write | Revoke one device |
| `POST` | `/v1/devices/{device_id}/grant` | signed | `ADMIN` | write | Give a pending device its first permissions (never `admin`) |
| `POST` | `/v1/agents/{pane_id}/respond` | signed | `RESPOND` | write | Submit a prompt |
| `POST` | `/v1/agents/{pane_id}/keys` | signed | `SEND_KEY` | write | Send allowlisted keys |
| `POST` | `/v1/agents/{pane_id}/stop` | signed | `RESPOND` | write | Stop a live agent (the pane leaves the list) |
| `POST` | `/v1/agents/{pane_id}/focus` | signed | `RESPOND` | write | Focus the pane |
| `POST` | `/v1/agents/{pane_id}/seen` | signed | `READ_STATE` | write | Clear the "done" badge |
| `POST` | `/v1/agents/{pane_id}/read` | signed | `READ_STATE` | **write** | Read recent pane output |
| `POST` | `/v1/agents/{pane_id}/image` | signed | `RESPOND` | write | Store an image and prompt the agent with its path |
| `POST` | `/v1/workspaces` | signed | `RESPOND` | write | Create a workspace and start a new opencode agent in it |
| `GET` | `/v1/chat/bootstrap` | signed | `READ_STATE` | read | Company, projects, seats, unread counters and chat revision |
| `GET` | `/v1/chat/conversations` | signed | `READ_STATE` | read | Recent conversations, search, pinned, section, `attention` and `working` filters |
| `GET` | `/v1/chat/conversations/counts` | signed | `READ_STATE` | read | Chip counts `all`, `attention`, `working`, `archived` (one query) |
| `POST` | `/v1/chat/conversations` | signed | `RESPOND` | write | Create a conversation (Alfred by default) |
| `GET` | `/v1/chat/conversations/{id}` | signed | `READ_STATE` | read | Conversation metadata and participants |
| `POST` | `/v1/chat/conversations/{id}/update` | signed | `RESPOND` | write | Rename, pin, move to a section or archive |
| `DELETE` | `/v1/chat/conversations/{id}` | signed | `RESPOND` | write | Delete the conversation for good; its notes stay (TASK-CONV-BORRAR) |
| `GET` | `/v1/chat/conversations/{id}/timeline` | signed | `READ_STATE` | read | Timeline page, newest last, by `seq` |
| `GET` | `/v1/chat/conversations/{id}/messages` | signed | `READ_STATE` | read | Raw message sync by `after_seq`/`before_seq` |
| `POST` | `/v1/chat/conversations/{id}/messages` | signed | `RESPOND` | write | Send a user message, optionally replying to one (`reply_to`) and carrying up to 4 `attachments`; idempotent by `client_id`; `202` |
| `POST` | `/v1/chat/conversations/{id}/read` | signed | `READ_STATE` | none | Move the read cursor to `last_seq` (non-destructive) |
| `POST` | `/v1/chat/conversations/{id}/attachments` | signed | `RESPOND` | write | Start an upload (`client_id`, `name`, `size`, `sha256`); `201` |
| `POST` | `/v1/chat/conversations/{id}/attachments/{att}/chunks` | signed | `RESPOND` | read | Append the next chunk at `offset` (base64); `409 offset_mismatch` carries `next_offset` |
| `POST` | `/v1/chat/conversations/{id}/attachments/{att}/complete` | signed | `RESPOND` | write | Check size and sha256, decode and re-encode as WebP, store; a failure drops the upload |
| `GET` | `/v1/chat/conversations/{id}/attachments/{att}` | signed | `READ_STATE` | read | Metadata of one attachment |
| `GET` | `/v1/chat/conversations/{id}/attachments/{att}/content` | signed | `READ_STATE` | read | The stored bytes of a ready attachment (`image/webp`) |
| `DELETE` | `/v1/chat/conversations/{id}/attachments/{att}` | signed | `RESPOND` | write | Drop an upload in progress or a ready file that no message took |
| `GET` | `/v1/chat/sections` | signed | `READ_STATE` | read | User sections and their order |
| `POST` | `/v1/chat/sections` | signed | `RESPOND` | write | Create or rename a section |
| `GET` | `/v1/chat/delegations/{id}` | signed | `READ_STATE` | read | Delegation, its children, its card and its result |
| `POST` | `/v1/chat/delegations/{id}/cancel` | signed | `RESPOND` | write | Cancel one delegation (the history stays) |
| `POST` | `/v1/chat/conversations/{id}/cancel` | signed | `RESPOND` | write | Kill switch: cancel every active delegation of the conversation |
| `POST` | `/v1/chat/conversations/{id}/stop` | signed | `RESPOND` | write | Stop the open turn of the conversation (`queued` or `running`); idempotent; `200 {"stopped","runs"}` |
| `GET` | `/v1/chat/approvals` | signed | `READ_STATE` | read | The UAC inbox: the approvals that wait for a decision |
| `POST` | `/v1/chat/approvals/{id}/decision` | signed | **`APPROVE`** | write | Allow or deny one approval, once |
| `POST` | `/v1/chat/approvals` | hook | — | write | Open a `pending` approval (the agent's own hook or plugin) |
| `GET` | `/v1/chat/approvals/{id}` | hook | — | read | The state of one approval (the plugin polls it) |
| `POST` | `/v1/chat/approvals/{id}/consume` | hook | — | write | Burn the approval just before the action runs |
| `GET` | `/v1/chat/approvals/settings` | signed | `READ_STATE` | read | The approval mode: the global one and the projects that override it (CORE9-D) |
| `POST` | `/v1/chat/approvals/settings` | signed | **`APPROVE`** | write | Set the mode, globally or for one project; audited and idempotent by `client_id` (CORE9-D) |

`Grant` is the per-device permission from [`auth.md`](auth.md). A new device is
pending (CORE11, D3): it holds no grant until the host grants one, so it reads
nothing either. An
authenticated device without the grant receives `403`, not `401`. An endpoint
added later without a matrix rule fails closed (`403`).

`read` is counted against the write budget on purpose: it drives work on the
host (a herdr round trip and a pane read) even though it mutates nothing.

`pane_id` must match `^[A-Za-z0-9]{1,32}:p[0-9A-Za-z]{1,32}$` (for example
`w1:p3`). A bare agent name is refused — targets are resolved only to panes the
host already mirrors.

## Request and response shapes

### `GET /v1/health` (no auth)

```json
{ "ok": true, "revision": 42, "devices": 2, "streams": 1 }
```

Nothing identifying: no herdr version, no pane names, no agent count.

### `GET /v1/state` → `state.md`

### `POST /v1/agents/{pane_id}/respond`

Body: `{ "text": "…" }` — non-empty, no control characters other than
newline/tab. There is **no length cap** (updated: the old 4000-character limit
and the `prompt_too_long` code are gone). Response: the summarised action result.

> **IMPLEMENTED.** The only remaining bound is the transport-signed-body cap of
> 256 KiB (`errors.md`), which is not a message-length rule; a longer message is
> only possible for the image route below, which raises its own cap.

### `POST /v1/agents/{pane_id}/keys`

Body: `{ "keys": ["enter"], "confirm": false }` — 1…8 keys.

- Allowed without `confirm`: `up down left right enter esc escape tab space
  backspace home end pgup pgdown y n` and the digits `0`–`9`.
- `ctrl+c` requires `confirm: true`.
- Anything else is refused. `escape` normalises to `esc`; keys are lower-cased
  and trimmed.

### `POST /v1/agents/{pane_id}/focus` / `.../seen`

No body. `focus` calls herdr's `agent.focus`. `seen` is an acknowledged no-op:
herdr 0.9.1 clears a "done" badge only through `agent.focus`, which would also
move the operator's terminal, so the phone marks the agent seen locally and the
host calls nothing (updated: this route no longer reaches herdr).

### `POST /v1/agents/{pane_id}/read`

Body: `{ "lines": 60 }` (optional). Clamped to 1…200, default 60. Response:

```json
{ "text": "…", "pane_id": "w1:p3" }
```

### `POST /v1/agents/{pane_id}/image`

Body: `{ "mime": "image/png", "data": "<base64>" }`.

- `mime` is allowlisted: `image/png`, `image/jpeg` or `image/webp`.
- `data` is standard base64 of the image bytes, at most 5 MiB decoded.
- The bytes must actually be the declared container (PNG magic, JPEG `FF D8 FF`,
  or a `RIFF….WEBP` header). A renamed file is refused, not stored.
- The host writes the file under its data dir (`--images`, default
  `~/.config/herdr-notify/images`) and submits the **absolute path** to the agent
  as a prompt (`Imagen adjunta: <path>`), so the agent opens it with its own
  tools. The file is removed again if herdr refuses the prompt.

Response:

```json
{ "ok": true, "pane_id": "w1:p3",
  "path": "/home/me/.config/herdr-notify/images/w1_p3-1a2b3c4d5e6f7890.png",
  "result": { "pane_id": "w1:p3", "agent": "opencode", "agent_status": "idle" } }
```

Failures: `400` with an action code (`unsupported_type`, `invalid_image`,
`empty_image`, `image_too_large`, `type_mismatch`), `403` without `RESPOND`,
`409` if herdr refuses the prompt, `413` if the signed body exceeds the image
cap. Requires `RESPOND`: attaching a picture drives the agent exactly as sending
a prompt does.

> **IMPLEMENTED.** The client side (`ImageAttachment`, `sendImage`) validates
> the same 5 MiB ceiling and the three types before sending.

### Action responses

Writes return `{"ok": true, "result": {…}}` where `result` is trimmed to what the
app needs (for a prompt: `pane_id`, `agent`, `agent_status`). herdr's own
refusal is a `409`, not a host fault (`errors.md`).

### `POST /v1/workspaces`

Body: `{ "agent": "opencode", "cwd": "<path>", "label": "…" }`.

- `agent` must name an entry of the host's own `agents.toml` (the closed list
  of commands the host is willing to start). The client never names an
  executable: an unknown name is `422 unknown_agent`, and a known name whose
  command does not resolve on this machine is `422 agent_unavailable` (both
  with `field: "agent"`, see `errors.md`).
- `cwd` must exist and be a directory; it is stored canonical. A path that is
  not is `422 invalid_cwd`.
- `label` is optional (≤ 64 characters, no control characters). It becomes
  the pane's display title and wins over the terminal's own OSC title. A
  label that breaks the rule is `422 invalid_label`.

Response (the host's shape):

```json
{ "ok": true, "workspace_id": "w1", "pane_id": "w1:p4",
  "agent": { "agent": "opencode", "agent_status": "unknown", "pane_id": "w1:p4" } }
```

A host that cannot start the configured command answers `409` with the reason.

### `GET /v1/catalog`

Response:

```json
{ "ok": true, "agents": [
  { "name": "opencode", "available": true, "label": "opencode" },
  { "name": "claude", "available": false, "label": "Claude Code" } ] }
```

Names and availability only: **never** the command, its arguments or a
resolved path. `available` says whether the command would resolve on this
machine (a file that exists, or a bare name found on `PATH`); `label` is a
display hint and falls back to `name`. Permission: `READ_STATE`.

The catalogue is re-read when `agents.toml` changes (its mtime), so entries
can be added, removed or edited without restarting the host; creation
(`POST /v1/workspaces`) sees the same file. A file that no longer parses is
reported in an additive `warning` string on this response, while the last good
catalogue keeps being served (creation keeps working with it). The warning
disappears once the file parses again.

### `GET /v1/roles`

The role catalogue: the professions a person can hire (ROLES §2, §4). It is
**not** `GET /v1/catalog`, which lists the CLIs this host can start. Read-only,
the same on every read, nothing from the host's state. Permission: `READ_STATE`.

Response (abridged):

```json
{ "ok": true, "version": 1,
  "areas": [
    { "id": "video", "label": "Vídeo y postproducción",
      "roles": ["vera", "jade", "karim", "leo", "priya", "sofia"],
      "levels": { "personal": ["vera"],
                  "profesional": ["vera", "jade", "karim", "leo"],
                  "equipo": ["vera", "jade", "karim", "leo", "priya", "sofia"] } } ],
  "roles": [
    { "id": "vera", "name": "Vera", "title": "Editor de vídeo", "area": "video",
      "avatar": null, "apps": ["FilmCraft", "SoundCraft"],
      "skills": { "ok": [], "pending": ["edit-rhythm", "subtitle-sync"] } } ] }
```

`levels` says which roles each onboarding level preselects (`personal` ⊆
`profesional` ⊆ `equipo` ⊆ the area's `roles`); `equipo` never exceeds 8.
`avatar` is the id of a character with artwork, or `null`; Alfred is never in
the catalogue. `skills.pending` lists skills the role still needs and does not
block it. `version` is the catalogue version, bumped when its shape changes.

### The team: `/v1/hires` and `/v1/onboarding` (ONBOARD-T3)

Contracting a professional only **registers** them: it creates no pane and no
seat, and no project. Every write is the user's `APPROVE`, which no agent holds.
At most 8 hires are active per area; a role that is already hired answers with
that same hire. Errors are in `errors.md` (“Hire and onboarding errors”).

A hire is `{id, role, area, name, avatar, status, created_at}`. `role` is the
catalogue id, `avatar` a character id or `null`, `status` is `hired` or
`removed`, and `created_at` is `%Y-%m-%dT%H:%M:%fZ`. The name is trimmed and is
1–40 characters with no control characters.

#### `GET /v1/hires`

Lists the hired professionals. `?include_removed=true` adds the dismissed ones.
`can_hire` is `true` when the calling device holds `APPROVE`, so the app shows
the team's buttons only then. It reports that grant and grants nothing new.

```json
{ "ok": true, "hires": [
  { "id": "hire_01JAB3ZQ5V8K2M9X4T6R0WQ7YD", "role": "bob", "area": "programacion",
    "name": "Bob", "avatar": "bob", "status": "hired",
    "created_at": "2026-10-25T10:00:00.000Z" } ], "can_hire": true }
```

#### `POST /v1/hires`

Body `{"role", "name"?, "avatar"?}`. The name defaults to the role's name and
the avatar to the role's own. `201` with the new hire; `200` with the hire the
role already has, unchanged (the body is then ignored).

```json
{ "role": "bob", "name": "Bob" }
```
```json
{ "ok": true, "hire": { "id": "hire_01JAB3ZQ5V8K2M9X4T6R0WQ7YD", "role": "bob",
  "area": "programacion", "name": "Bob", "avatar": "bob", "status": "hired",
  "created_at": "2026-10-25T10:00:00.000Z" } }
```

#### `POST /v1/hires/{hire_id}/update`

Body `{"name"?, "avatar"?}`. The fields tell three things apart:

- **absent** (`name` or `avatar` not sent): that field stays as it is;
- **`"avatar": null`**: the hire loses its character and shows the initial
  (`avatar` becomes `null`); the name stays;
- **a string** (`"avatar": "ines"`): that character, which must be one of the 8
  (anything else is `422 invalid_field`, field `avatar`).

A body with neither `name` nor `avatar` is `422 invalid_field`: there is nothing
to change. Name and avatar are both checked before anything is written. A
dismissed hire answers `409 hire_not_active`, also when only the avatar is sent.

```json
{ "name": "Roberto", "avatar": "ines" }
```
```json
{ "avatar": null }
```
```json
{ "ok": true, "hire": { "id": "hire_01JAB3ZQ5V8K2M9X4T6R0WQ7YD", "role": "bob",
  "area": "programacion", "name": "Roberto", "avatar": "ines", "status": "hired",
  "created_at": "2026-10-25T10:00:00.000Z" } }
```

#### `POST /v1/hires/{hire_id}/dismiss`

No body. The hire becomes `removed` and stops counting against its area's
limit. Dismissing it again answers the same hire.

```json
{ "ok": true, "hire": { "id": "hire_01JAB3ZQ5V8K2M9X4T6R0WQ7YD", "role": "bob",
  "area": "programacion", "name": "Roberto", "avatar": "ines", "status": "removed",
  "created_at": "2026-10-25T10:00:00.000Z" } }
```

#### `GET /v1/onboarding`

`done` is `false` until the first `POST`; `areas` are the areas it chose.
`can_hire` is `true` when the calling device holds `APPROVE`, the same grant
`GET /v1/hires` reports. A device without it sees `false` and gets `403` on
every write of the team.

```json
{ "ok": true, "done": true, "areas": ["video"], "can_hire": true }
```

#### `POST /v1/onboarding`

Body `{"areas": [area_id], "level"?: "personal" | "profesional" | "equipo",
"roles"?: [role_id], "skip"?: boolean}`. Each area is one of the catalogue's
areas, and at least one is needed.

- With `"skip": true` and no `areas` or `roles`, the onboarding is marked done
  and nobody is contracted: `hires` is `[]`. A second skip changes nothing, and
  a skip after a finished onboarding keeps its hires. `skip` with `areas` or a
  non-empty `roles` is `422 invalid_field` on `skip`. Without `skip` (or with
  `false`) the rules below stand.

- With `roles`, exactly those roles are contracted and `level` is not used. Each
  role must belong to one of the chosen areas.
- Without `roles`, each area contributes its list for `level` (`profesional`
  when omitted; see the `levels` of `GET /v1/roles`).

Everything is checked before anything is contracted. The team is contracted in
one transaction, so a refused limit leaves no hire behind. The onboarding is
then marked done. A second `POST` contracts nothing new. An unknown `level`
value is `422 invalid_body`.

```json
{ "areas": ["video"], "level": "personal" }
```
```json
{ "ok": true, "done": true, "hires": [
  { "id": "hire_01JAB3ZR1C7N4P0E8F2G5H6J9K", "role": "vera", "area": "video",
    "name": "Vera", "avatar": null, "status": "hired",
    "created_at": "2026-10-25T10:02:00.000Z" } ] }
```

Skipping (the «Saltar» of the first-run screen):

```json
{ "skip": true }
```
```json
{ "ok": true, "done": true, "hires": [] }
```

### `POST /v1/agents/{pane_id}/stop`

No body. Permission `RESPOND`. The host closes the agent's process (an
orderly signal, forced after two seconds), the pane leaves the state list and
`pane.closed` goes out on the stream. Every later action on it answers
`pane_gone` — so stopping a pane that is already gone is `pane_gone` too.
A pane restored from a previous run has no process to stop and answers `409`
(`no running agent`). Response: `{"ok": true, "pane_id": "w1:p4"}`.

### `POST /v1/hooks/report` (host-local; not a device route)

Agent hooks — the agent's own plugins, running as local processes — report
`{ "pane_id": "w1:p4", "state": "blocked", "message": "…" }` with a hook token
in `X-HN-Hook-Token`: the host token (`host hook-token`) or the per-run token
the host handed to the agent. Only loopback is accepted, and this is not part
of the signed device surface: a device signature is never accepted here, and
the hook token is never accepted on the device routes. A fresh report wins
over screen-pattern detection until its TTL passes. See
[`../core/HOOKS.md`](../core/HOOKS.md).

### `POST /v1/pair` (no auth)

Body: `{ "curve": "p256", "public_key": "<b64>", "label": "Pixel 9", "code": "HJKQ2ZP4" }`.
`code` is optional; `label` is optional. Response:

```json
{
  "ok": true,
  "device_id": "dev-…",
  "curve": "p256",
  "label": "Pixel 9",
  "permissions": [],
  "owner_id": "12345",
  "recovery_code": "7QK3-M2XP-9B7C-D4HF"
}
```

`recovery_code` is shown **once**; only a hash is stored. Failures answer
`403 {"detail":"pairing_rejected"}` (uniform), `400 {"detail":"invalid_body"}`,
`413`, or `429`. The exact resolution order (code → recovery → same-owner) is in
[`pairing.md`](pairing.md).

### `POST /v1/pair/pake/start` and `/finish` (no auth, TLS listener only; F2b, alpha)

Short-code pairing bound to the TLS connection. `start` takes `{"device_pubkey": "<b64>", "message": "<b64>"}` and answers
`{"message": "<b64>", "tag": "<b64>"}`; `finish` takes `{"tag": "<b64>"}` and answers like `POST /v1/pair` (a pending device with
no permissions, its `recovery_code` once). A request on the loopback router has no TLS exporter: `403 pake_requires_tls`.
Failures: `400 invalid_body` / `pake_invalid_message`, `403 pake_requires_tls` / `confirmation_failed`, `410 no_live_code` /
`pake_session_gone`, `413`, `429`. Flow, formats, what each tag binds and the audit are in
[`pairing.md`](pairing.md#f2b--pake-endpoints-alpha-no-client-yet--implemented-in-the-host-no-client-f2d).

### `POST /v1/admin/pake-code` (host-local; not a device route)

Arms the one PAKE code and answers `{"code": "ABCDEFGH", "expires_in": 180}`. Only the loopback router mounts it (`404` on
the LAN listener), only the host hook token in `X-HN-Hook-Token` opens it (`401` without or with a wrong one, `403
host_token_required` for a per-run token), `409 pake_unavailable` when the host has no TLS identity. `host pair-code --pake`
is its client. The response is the only place the code appears; it is never logged or written.

### `GET /v1/devices` (signed, `ADMIN`)

```json
{"ok": true, "devices": [
  {"device_id":"dev-…","curve":"p256","label":"Pixel 9","owner_id":"12345",
   "created_at":1730000000.0,"last_seen_at":1730000000.0,"revoked":false,
   "permissions":["read_state"]}
]}
```

Public material only: never a `public_key` or an `fcm_token`.

A device that holds no permission and is not revoked (pending) also carries
`"sas": "097665"`: the six-digit verification string the phone shows, computed
as in [`pairing.md`](pairing.md#verification-string-sas--implemented-f1-núcleo).
Other devices have no `sas` field. The field is absent when the host has no pin
to compute it.

### `POST /v1/devices/{device_id}/revoke` (signed, `ADMIN`)

No body. Response `{"ok": true, "device_id": "dev-…", "revoked": true}`, or
`404 {"detail":"no such device"}`. After a revoke the device's signed requests
answer `401` and its live stream closes within one keepalive.

### `POST /v1/devices/{device_id}/grant` (signed, `ADMIN`, F1-núcleo)

Body: `{"permissions": ["read_state", "respond"]}`: one or more of `read_state`,
`respond`, `send_key`, `approve` (the labels are case-insensitive). Response
`200` with the device, in the same shape as an element of `GET /v1/devices`, its
permissions now set. The grant is recorded in the pairing audit as a `granted`
line and announced by Alfred (see [`pairing.md`](pairing.md)).

Only a **pending** device can be granted here: one that holds no permission and
is not revoked. Errors (see [`errors.md`](errors.md)):

| Status | `detail` | When |
|--------|----------|------|
| `400` | `{"code": "admin_not_grantable", …}` | `admin` in the list; ADMIN is granted by `host devices grant` only |
| `400` | `{"code": "cannot_grant_self", …}` | the path names the calling device |
| `400` | `"invalid_body"` | not JSON |
| `404` | `"no such device"` | unknown or revoked device |
| `409` | `{"code": "not_pending", …}` | the device already holds a permission |
| `422` | `{"code": "invalid_body", "field": "permissions", …}` | the list is empty or names an unknown permission |
| `403` | `"forbidden"` | the caller lacks `ADMIN` |

### `/v1/chat` (persistent chat, CORE6A/CORE6B/CORE7)

Implemented by `crates/chat` (SQLite `chat.db`) and
`crates/api/src/routes/chat`. The approval routes are CORE8 (Hito 3): they are
implemented, with the two surfaces described below.
`GET /v1/chat/bootstrap`:

```json
{"ok":true,"company":{"id":"co_01J…","name":"Mi empresa"},
 "projects":[],
 "alfred":{"agent_id":"ag_01J…","name":"Alfred","status":"draft"},
 "unread":{"conversations":0,"messages":0,"approvals":0},
 "chat":{"revision":0,"unread":0,"pending_approvals":0,"pending_user_approvals":0,"active_delegations":0}}
```

The `chat` block carries the monotonic chat revision, the unread message count
and the counters for the future approval/delegation surfaces. CORE6B adds the
same block to `/v1/state` and to the opening `event: state` frame of
`GET /v1/events`; bootstrap and both read models share one builder.

`GET /v1/chat/conversations?q=&section_id=&pinned=&archived=&attention=&working=&cursor=&limit=`
lists Recientes: `pinned=false` means "no filter", `archived=false` hides
archived chats, `limit` defaults to 30 (1…100). `q` searches titles and message
bodies (FTS5 when the SQLite build has it, `LIKE` otherwise; the choice is
recorded in `meta.search_mode` and is not part of the wire contract). All
parameters combine, and `cursor` pages the filtered list without repeats.

`attention=1` (or `true`) keeps the chats that wait for the operator: an agent
reply the operator has not read (`unread > 0`), or an approval of that chat
still pending at the operator (`level = "user"`, the same notion as
`pending_user_approvals`). Approvals that wait at a lead or at the CTO do not
count. `working=1` keeps the chats with a turn `queued` or `running`, the rule
of `active_runs`. Any other value of either flag, `0` and `false` included, is
`422 invalid_body` with `detail.field` set to the parameter name. Absent means
no filter, so clients that predate the flags read the same list.

`GET /v1/chat/conversations/counts` answers the numbers of the chips with the
same rules and no parameters: `{"ok":true,"all":N,"attention":N,"working":N,"archived":N}`.
`all`, `attention` and `working` skip archived chats, as the default listing
does; `archived` counts the archived ones. It is one aggregate query. The chip
numbers are not filtered by `q`, `section_id` or `pinned`. The SSE frames do not
change: clients recount on `chat.conversation` and `chat.message`.

Response:

```json
{"ok":true,
 "conversations":[{"id":"cnv_01J…","title":"Plan semanal","pinned":false,
   "archived":false,"last_message_at":"2026-10-21T10:00:00.000Z",
   "last_seq":6,"last_read_seq":0,"unread":3,
   "created_at":"…","updated_at":"…"}],
 "next_cursor":"cnv_01J…"}
```

`project_id` and `section_id` appear only when set. `active_runs` counts the
turns of the conversation in `queued` or `running` state (delegations included)
and is omitted when it is 0; clients that predate it read it as absent.
`progress` is the line Alfred set with `chat.progress` on the latest open turn of
the conversation (see [Chat progress](#chat-progress-chat-core-progress)). It is
omitted when no turn is open or none set a line. The listing and the detail carry
it; the bootstrap has no conversation rows and does not. `POST
/v1/chat/conversations` takes `{"title":"…","project_id":"prj_…"}` (both
optional; a blank title becomes `Nuevo chat`) and answers
`{"ok":true,"conversation":{…}}`. An unknown project is `422 unknown_project`.

`GET /v1/chat/conversations/{id}` answers
`{"ok":true,"conversation":{…},"participants":[{"agent_id":"…","name":"Alfred","status":"draft"}]}`.

`POST /v1/chat/conversations/{id}/update` takes any subset of
`{"title","pinned","section_id","archived"}`. `section_id: null` clears the
section; an absent field keeps it. Response: `{"ok":true,"conversation":{…}}`.

`DELETE /v1/chat/conversations/{id}` deletes the conversation for good. The app
asks the user first; the signed request is that answer, so no approval card is
opened. Response: `200 {"ok":true}`. In one transaction it removes the messages,
the attachments (rows and files), the delegations and their cards, the mail
draft and sign-in cards, the hook tokens and the journal frames of the
conversation, and its search row. Nothing is cancelled: an open turn, delegation
or pending approval refuses it.

| Status | `detail` | When |
|--------|----------|------|
| `404` | `"no_such_conversation"` | unknown id, or a second delete of the same id |
| `409` | `"conversation_busy"` | a turn is `queued` or `running`, a delegation is `queued`, `running` or `waiting`, or an approval of the conversation is `pending`; stop or resolve it, then delete |

What stays: the notes (`note.*`), whose text is kept and whose `conversation_id`
and `message_id` become `null`; the resolved approvals, without their
`conversation_id` and `delegation_id`, so their audit trail stays whole; and the
`agenda_audit` line `object_kind: "conversation"`, `action: "delete"`, with the
device that asked. The audit has no title and no content.

The other devices learn of it from the journal: a `chat.conversation` frame with
`change: "deleted"` and the `conversation_id` in its body. Its journal row has no
`conversation_id` column, because no row of a deleted conversation may remain, so
the frame reaches the clients that replay after their last id.

`GET …/timeline?before_seq=&limit=` pages newest-last by `seq`, default 50.
Mail draft cards are `kind: "mail_draft"` entries (`card_id`, `account`,
`draft_id`, `to`, `subject`, `preview`); they reach only devices with `READ_STATE`.
See `mail.md`.
(1…200). Entries carry `seq`, `kind`, `message_id`, `author_kind`,
`author_agent_id` (agents only), `preview` (one line, ≤120 chars), `state` and
`created_at`; `next_before_seq` is the cursor for the next older page. A
message entry has `kind: "message"`; a delegation has `kind: "card"` with
`card_id`, `delegation_id`, `ref_id` (the delegation id), `title`, `summary`
(one line), `status`, `agent_id`, `pane_id`, `attention` and `links` (see
[Delegations and cards](#delegations-and-cards-core7)).

Message entries carry three optional fields, all omitted when empty so older
clients see the same JSON as before:

- `turn_state` (user messages only): `queued`, `running`, `completed`, `failed`
  or `interrupted`, read from the turn of that message. `running` means the
  agent has taken the turn, not that it has read the message. Messages from
  before the turn existed have no `turn_state`.
- `suggestions` (agent messages only): up to 4 short replies, each 1…60
  characters with no control characters. Absent when there are none.
- `quote`: `{"kind":"message","id","author_kind","preview"}` for a reply to
  another message of the same conversation. `preview` is at most 120
  characters with collapsed spaces. It is resolved when the page is read, so it
  follows the original; when the original is missing or belongs to another
  conversation, `quote` is absent.

```json
{"seq":12,"kind":"message","message_id":"msg_01J…","author_kind":"user",
 "turn_state":"running","preview":"Crea el proyecto Demo","state":"complete"}
{"seq":13,"kind":"message","message_id":"msg_01K…","author_kind":"agent",
 "suggestions":["Sí, créalo","Cámbialo a Beta"],"preview":"¿Qué nombre?"}
{"seq":14,"kind":"message","message_id":"msg_01M…","author_kind":"user",
 "quote":{"kind":"message","id":"msg_01K…","author_kind":"agent",
   "preview":"¿Qué nombre?"},"preview":"Sí, créalo"}
```

`suggestions` has no write route in this contract: the agent writes them
through `chat.reply`. `POST …/messages` takes an optional `reply_to` (below);
an unknown field in a request body is ignored.

`GET …/messages?after_seq=&before_seq=&limit=` is the raw sync: full `body_md`,
oldest-first when `after_seq` is present, newest-last otherwise (default 100,
1…200).

`POST …/messages` is the turn:

```json
{"client_id":"6d0f…","text":"Crea el proyecto Demo"}
```

`client_id` is required (1…128 characters, no controls), `text` is trimmed,
non-empty, no control characters other than newline/tab, and at most 32 KiB of
UTF-8.

```json
{"client_id":"9b1c…","text":"Sí, créalo","reply_to":{"kind":"message","id":"msg_01K…"}}
```

`reply_to` is optional and answers a message of the same conversation. v1 takes
only `kind: "message"`; any other kind is `422 unsupported_reply_kind` with
`field: "reply_to"`. An `id` that is not a message of this conversation (another
conversation or an unknown id) is `422 unknown_reply_to` with `field:
"reply_to"`. A refused reply stores nothing. `reply_to: null` is the same as
no `reply_to`. A request without `reply_to` is an old client's: the message is
stored without a `quote` and the answer is unchanged. The `quote` of the reply
appears in the timeline (see above). The answer is `202` because delivery is
asynchronous:

```json
{"ok":true,
 "message":{"id":"msg_01J…","conversation_id":"cnv_01J…","seq":12,
   "author_kind":"user","body_md":"…","state":"complete","created_at":"…"},
 "turn":{"id":"run_01J…","state":"queued","agent_id":"ag_01J…"}}
```

Replaying the same `client_id` returns the first message and turn and does not
deliver the turn again (the same holds if the retry carries different text or a
different `reply_to`: the stored original wins, and the `reply_to` is not
checked against it). A retry with a `reply_to` of another kind is still `422`,
because the request shape is checked before the stored turn is looked up. `202` means the message and the `queued` turn are
durable. The host's seat owns the rest of the lifecycle:
`queued → running → completed | failed | interrupted`. `exited` is accepted as
a legacy alias of `completed` when reading an old database.

If no supported CLI is installed (or its version is too old, or `hn-mcp` is
missing), the send is refused with `409` and `detail: "seat_unavailable"`. If
the seat's pending queue is full, it is `409` and `detail: "agent_busy"` with
`Retry-After: 5`. In both cases the turn was not delivered: its message and
its run are withdrawn from the conversation, so nothing of it stays in the
timeline. Sending the same text again with the same `client_id` is accepted
once the seat is free (it is not a duplicate, because the first copy was
withdrawn).

A CLI that exits or exceeds its turn timeout without calling `chat.reply` ends
`failed`; the host appends an empty agent message as the failure card. It never
invents a reply. The run's `reason` is the cause (`process_exit: <code>`,
`turn_timeout`, …) followed by `; última salida: ` and at most 512 characters of
the CLI's last output (stdout and stderr, last 4 KiB read, one line). That excerpt
is redacted first: the run's tokens, the message text wherever the CLI echoes it
verbatim, and sensitive `key=value` pairs are replaced, and control characters
are removed. The host logs the same
reason. The prompt carries only the newest 20
messages and at most 24 000 characters of the conversation, with a truncation
notice; older history is never dumped into the CLI. `chat.ask_user` appends a
full agent message but does not close the turn: a successful turn still ends
with `chat.reply`. Writing to an archived conversation is `409
conversation_archived`.

`POST …/read` takes `{"last_seq":12}` and moves the cursor forward, never
backwards and never past the last message; response
`{"ok":true,"conversation_id":"cnv_…","last_read_seq":12}`.

`GET /v1/chat/sections` answers `{"ok":true,"sections":[{"id":"sec_…","name":"Trabajo","order":1}]}`.
`POST /v1/chat/sections` takes `{"name":"Trabajo"}` to create or
`{"id":"sec_…","name":"Trabajo profundo"}` to rename; response
`{"ok":true,"section":{…}}`.

### Attachments (ATT4/ATT5)

A user can attach up to four images to a message. The host checks the bytes it
receives, decodes each image in an isolated worker and keeps only the result it
re-encodes. The six routes are signed and scoped to their conversation: an id of
another conversation gets the same `404` as an unknown one.

Order of use: `POST …/attachments`, then `POST …/{att}/chunks` until every byte
is sent, then `POST …/{att}/complete`, then `POST …/messages` with
`attachments`. A file that a message has taken is no longer deletable.

Create (`RESPOND`, write budget):

```json
{"client_id":"5e2f…","name":"captura.png","size":183204,"sha256":"9f86d081…"}
```

`201 {"ok":true,"attachment":{"id":"att_01K…","state":"uploading"}}`

- `client_id`: 1…128 characters, no control characters. It is the idempotency
  key: a repeat answers `201` with the first attachment; its `name` and `size`
  are not compared.
- `name`: 1…120 characters, no control characters. The user sees it; it never
  reaches the disk or the agent's prompt.
- `size`: the input's size in bytes, at most 10 MiB (10 485 760). It counts
  against the quotas from this call on.
- `sha256`: 64 hex characters, either case; stored in lowercase.

Chunk (`RESPOND`, read budget: 120/min; a 10 MiB file is 80 chunks):

```json
{"offset":131072,"data":"iVBORw0KGgo…"}
```

`200 {"ok":true,"next_offset":262144}`

- `offset`: must equal the number of bytes already stored.
- `data`: standard base64 of at most 128 KiB (131 072) raw bytes. The JSON body
  is about 175 KB, under the 256 KB signed-body cap.
- A wrong `offset` answers `409 offset_mismatch` with the position to resume
  from:

```json
{"detail":{"code":"offset_mismatch","message":"…","next_offset":131072}}
```

Complete (`RESPOND`, write budget, no body):

`200 {"ok":true,"attachment":{"id":"att_01K…","name":"captura.png","mime":"image/webp","size":48212,"w":1024,"h":768,"state":"ready"}}`

The host checks that the stored bytes have the declared size and sha256, decodes
the image in a worker process with a 5 s timeout, and re-encodes it as WebP with
the longest side at most 2048 px and no metadata. A `ready` attachment answers
`200` again with the same body.

**A failed `complete` drops the upload**: its row and its bytes are removed, so
later calls answer `404 no_such_attachment` and the client starts again with
`POST …/attachments`.

Read:

- `GET …/attachments/{att}` answers `200 {"ok":true,"attachment":{"id","name","mime","size","w","h","state"}}`.
  `mime`, `size`, `w` and `h` are `null` while the state is `uploading`.
- `GET …/attachments/{att}/content` answers the bytes with the headers
  `Content-Type: image/webp`, `X-Content-Type-Options: nosniff`,
  `Content-Disposition: attachment` and `Cache-Control: private, no-store`.
  A file that is not ready answers `409 not_ready`.
- `DELETE …/attachments/{att}` answers `200 {"ok":true}`. It drops an upload in
  progress or a ready file that no message has taken.

Send (`RESPOND`, write; see `POST …/messages` above):

```json
{"client_id":"c3…","text":"","attachments":["att_01K…"]}
```

- `attachments`: 0 to 4 ids of ready, unsent attachments of this conversation.
  Absent means none.
- An id that is unknown, of another conversation, not ready or already sent is
  `422 unknown_attachment`; more than four is `422 too_many_attachments`. Both
  carry `field: "attachments"`. The message and its links are written in one
  transaction, so a refusal stores nothing.
- Empty `text` is allowed only with at least one attachment; the stored body is
  `(adjunto)`. Without attachments the text rules above apply unchanged.
- The timeline shows the files of a user message in send order, in
  `attachments` (omitted when empty):
  `{"id":"att_…","name":"captura.png","mime":"image/webp","size":48212,"w":1024,"h":768}`.
  The raw sync `GET …/messages` does not carry them.

#### Attachment errors

| HTTP | `detail` | When |
|------|----------|------|
| `400` | `"invalid_body"` | the body is not JSON |
| `422` | `{ "code": "invalid_body", "errors": […] }` | JSON of the wrong shape (a missing or mistyped field) |
| `422` | `{ "code": "invalid_body", "field": "client_id", … }` | `client_id` empty, over 128 characters, or with a control character |
| `422` | `{ "code": "invalid_name", "field": "name", … }` | `name` empty, over 120 characters, or with a control character |
| `422` | `{ "code": "invalid_sha256", "field": "sha256", … }` | `sha256` is not 64 hex characters |
| `422` | `{ "code": "invalid_base64", "field": "data", … }` | `data` is not base64 |
| `422` | `{ "code": "size_mismatch", "field": "size", … }` | `complete`: the stored bytes are not the declared size |
| `422` | `{ "code": "hash_mismatch", "field": "sha256", … }` | `complete`: the stored bytes do not match the declared sha256 |
| `422` | `{ "code": "unsupported_type" \| "invalid_image" \| "image_too_large" \| "dimensions_too_large" \| "input_too_large" \| "timeout", "field": "file", … }` | `complete`: the image is refused or took more than 5 s |
| `422` | `{ "code": "unknown_attachment", "field": "attachments", … }` | a message names an id that is unknown, of another conversation, not ready or already sent |
| `422` | `{ "code": "too_many_attachments", "field": "attachments", … }` | a message names more than four ids |
| `404` | `"no_such_attachment"` | unknown id, or an id of another conversation |
| `404` | `"no_such_conversation"` | the conversation is unknown to this host |
| `409` | `"conversation_archived"` | an upload or a message in an archived conversation |
| `409` | `{ "code": "not_uploading", … }` | a chunk to an attachment that is not uploading; a `complete` whose upload changed state under it |
| `409` | `{ "code": "offset_mismatch", "next_offset": N, … }` | the chunk's `offset` is not the stored byte count |
| `409` | `{ "code": "not_ready", … }` | `content` of an attachment that is not ready |
| `409` | `{ "code": "already_linked", … }` | a `DELETE` after a message took the file |
| `413` | `{ "code": "file_too_large", … }` | `create`: `size` over 10 MiB; a chunk whose end passes the declared size |
| `413` | `{ "code": "chunk_too_large", … }` | a chunk over 128 KiB of raw bytes |
| `413` | `{ "code": "conversation_quota", … }` | `create`: the conversation would keep more than 100 MiB of attachments |
| `413` | `{ "code": "host_quota", … }` | `create`: the host would keep more than 1 GiB of attachments |
| `429` | `"slow down"` | the read or write budget of the route (`Retry-After`) |
| `500` | `"internal error"` | a storage or database fault, or the image worker failed (`worker_failed` is logged, not sent) |

Coded refusals keep the `detail.code` and `detail.message` shape. The `404`s and
`409 conversation_archived` answer `detail` as a plain string, as the other
`404`s and the archived refusal of this surface do.

#### Client rules

- Send the file in chunks of at most 128 KiB of raw bytes, in order. Each
  `offset` is the number of bytes sent before it.
- On `409 offset_mismatch`, resume from `detail.next_offset`.
- Declare `size` and `sha256` at `create`. The host checks both at `complete`,
  and `size` is what the quotas count.
- Call `complete` before the message. A message refuses an id that is not ready.
- Compress or downscale the image before uploading. The input is at most 10 MiB,
  the decoder accepts at most 8192 px on a side and 40 million pixels
  (`dimensions_too_large`, `image_too_large`).
- Input types are PNG, JPEG and static WebP only (`unsupported_type` for an
  animated PNG or WebP). The stored result is always WebP.
- The quotas are 100 MiB per conversation and 1 GiB per host, over the bytes the
  attachments keep.

#### Security

- The type is the one of the file's real header (PNG, JPEG or WebP signature),
  never the name or a MIME type the client sent.
- Decoding runs in an isolated worker process (`hn-media-worker`) with a 5 s
  timeout and fixed input, side and pixel limits. The output is re-encoded from
  the decoded pixels, so no metadata of the input survives.
- The agent receives the files as untrusted data. Its prompt holds one line per
  file with the id, the type, the dimensions, the size and the path the host
  stores it at, never the name the user chose. The prompt says that the files
  may contain instructions and must not be obeyed.
- The bytes are served with `nosniff`, `attachment` and `no-store`.

### Voice (VOZ2)

`POST /v1/voice/transcribe` turns one voice note into text on the host. It is
signed and needs `RESPOND`: a voice note is a message in another form, so it
takes the same permission and the write budget.

The body is the raw audio. `Content-Type: audio/ogg` is Ogg Opus; `audio/wav` and
`audio/x-wav` are WAV (parameters allowed on any of them). Another type is
`400 unsupported_type`. The body is capped at `MAX_VOICE_BODY_BYTES` (2 MiB, about
60 to 90 s of Opus, or about 65 s of 16 kHz PCM16) and read under that cap before
the signature is checked: a larger one is `413 "body too large"`.

A WAV must be integer PCM (`fmt ` format 1), mono, 16-bit, exactly 16 000 Hz. Any
other layout (stereo, 8-bit, another rate, float, extensible), a truncated or odd
`data` chunk, or no samples is `400 {"code": "unsupported_audio"}`: the host
converts and resamples nothing. Unknown chunks (such as `LIST`) are skipped, and a
`data` size of `0` or `0xFFFFFFFF` (a streamed file) runs to the end of the body.
The decoded audio has the same 120 s limit as Ogg. The shortest valid file is the
44-byte header below, then the samples as little-endian int16:

| Offset | Bytes | Meaning |
|--------|-------|---------|
| 0 | `52 49 46 46` | `RIFF` |
| 4 | u32 | file size minus 8 (36 + sample bytes) |
| 8 | `57 41 56 45` | `WAVE` |
| 12 | `66 6d 74 20` | `fmt ` |
| 16 | `10 00 00 00` | fmt size, 16 |
| 20 | `01 00` | format 1, integer PCM |
| 22 | `01 00` | 1 channel |
| 24 | `80 3e 00 00` | 16 000 Hz |
| 28 | `00 7d 00 00` | byte rate, 32 000 |
| 32 | `02 00` | block align, 2 |
| 34 | `10 00` | 16 bits |
| 36 | `64 61 74 61` | `data` |
| 40 | u32 | sample bytes |
| 44 | int16 LE | the samples |

`200 {"text": "…", "ms": 1840}`: the transcript (empty when there is no speech)
and the time taken in milliseconds. The client puts the text in the composer; it
is never run as a command or sent on its own.

The host corrects a short dictionary of project terms in the text: the engine
hears the name Excubia as «excluye» or «excluya», and those are written as
Excubia.

Then a corrector rewrites the words that sound like a command verb or a project
term («aprovevalo» → «apruébalo», «scubia» → «Excubia»). It is deterministic and
local: no model and no network. It compares the Spanish sound of runs of one to
three words with a closed list of 37 entries, changes a run when the relative
distance is at most 0.25, and never changes a common word (so «borré» or «para»
stay). Capitals and punctuation stay. The response carries the corrected text;
the uncorrected text is not kept anywhere. It is on by default. Start the host
with `--asr-no-corrector` to get the recognizer's text as it is.

Nothing is stored: neither the audio nor the text reaches the disk or the log.
The log keeps the size, the duration and the levels of the audio of each request
(peak, rms, clipping, speech time, noise floor), never the audio or the text.

One transcription runs at a time. A second request while one runs is
`429 voice_busy` with `Retry-After: 2`; there is no queue.

The engine is a worker process (Canary, language `es`, CUDA only) that the host
starts when the host starts (or on the first request if that failed); loading takes up to 40 s, and a note sent meanwhile is `429 voice_busy`. If the worker dies or times
out, the next request starts it again. After three failed starts in a row the
route answers `503` until the host restarts. The engine comes from
`--asr-python`, `--asr-script` and `--asr-model`, which go together; without them
the route answers `503 voice_unavailable` and the host starts anyway.
Whisper large-v3 is the other engine (`worker/asr_worker_whisper.py`, same protocol and
flags): `--asr-script <ruta>\crates\asr\worker\asr_worker_whisper.py --asr-model <carpeta del
snapshot faster-whisper-large-v3> --asr-python <venv>`. The venv needs `faster-whisper==1.2.1`
and the `nvidia-*` CUDA wheels (cuBLAS, cuDNN). The default engine is whichever script the
arguments name; Canary is `asr_worker.py`.
Qwen3-ASR 1.7B is the third (`worker/asr_worker_qwen.py`, same protocol and flags):
`--asr-script <ruta>\crates\asr\worker\asr_worker_qwen.py --asr-model <carpeta de la instantánea
Qwen/Qwen3-ASR-1.7B, revisión 7278e1e7> --asr-python <venv>`. The venv needs `qwen-asr==0.0.6`,
`transformers==4.57.6` and `torch==2.11.0` with CUDA 12.8 (reference:
`research\voz-spike\venv-qwen-cu128`). The model is Apache-2.0 and uses about 4.7 GB of VRAM in bf16.

Errors: see [`errors.md`](errors.md#voice-errors-voz2).

> **IMPLEMENTED.** The Opus decoding runs in the host's own process (pure Rust,
> about 250 `unsafe` blocks). Isolating it in a separate process is a security
> debt to pay before the host is opened beyond the LAN.

### Delegations and cards (CORE7)

A delegation is a task one seat gives to another: Alfred to the lead of a
project (depth 1) and the lead to a specialist of the same project (depth 2).
Its state is `queued | running | waiting | done | failed | cancelled`
(`interrupted` after a restart). Every delegation owns one timeline card, so
the timeline mixes `kind: "message"` and `kind: "card"` entries in the same
`seq` space.

`GET /v1/chat/delegations/{id}` answers the whole record:

```json
{"ok":true,
 "delegation":{"id":"dlg_01J…","conversation_id":"cnv_01J…","project_id":"prj_01J…",
   "parent_id":null,"from_agent_id":"ag_01J…","from_name":"Alfred",
   "to_agent_id":"ag_01J…","to_name":"Ernesto","task":"Planifica",
   "state":"done","depth":1,"turn_key":"run_01J…","pane_id":"w1:p3",
   "result_message_id":"msg_01J…","reason":null,
   "created_at":"…","updated_at":"…","finished_at":"…"},
 "children":[{"id":"dlg_01J…","depth":2}],
 "card":{"id":"act_dlg_01J…","seq":3,"title":"Alfred pidió a Ernesto",
   "summary":"Ernesto terminó","status":"done","agent_id":"ag_01J…",
   "pane_id":"w1:p3","attention":0,
   "links":{"agent":"/v1/agents/w1:p3","delegation":"/v1/chat/delegations/dlg_01J…"}},
 "result_message":{"id":"msg_01J…","body_md":"…","author_kind":"agent"},
 "synthesis":{"state":"queued","run_id":"run_01J…"}}
```

`links.agent` points at the pane that owns the task (the same agent detail the
client already has) and `links.delegation` at this record; the card never
duplicates the agent detail. `children` are the delegations made under this
one. `synthesis` is the state of Alfred's coalesced summary of the result
(`pending | queued | skipped`).

An unknown delegation is `404 no_such_delegation`.

`POST /v1/chat/delegations/{id}/cancel` cancels one delegation: it answers the
same record with `state: "cancelled"`, the card keeps its place in the timeline
and nothing is deleted. Cancelling an already finished delegation is `409
delegation_cancelled`. The durable state is written first and the panel is
interrupted afterwards, so a cancelled delegation never comes back.

`POST /v1/chat/conversations/{id}/cancel` is the kill switch of a
conversation: every active delegation (`queued | running | waiting`) is
cancelled and the answer is `{"ok":true,"conversation_id":"cnv_…","cancelled":
["dlg_…"]}`. The conversation and its history stay.

`POST /v1/chat/conversations/{id}/stop` stops the open turn of Alfred.
The answer is `200 {"ok":true,"stopped":true,"runs":N}` when a turn was open,
and `200 {"ok":true,"stopped":false,"runs":0}` when none was: a second stop is
never an error. An unknown conversation is `404 no_such_conversation`; a device
without `RESPOND` is `403` and changes nothing.

- Order: the store goes first. Every `queued` or `running` run of the
  conversation becomes `interrupted` (reason `stopped_by_user`) in one
  transaction, and its user message journals `turn_state: "interrupted"` in an
  `updated` event. Then the seat kills the CLI process of each running turn; a
  queued turn is never started (the worker skips it). A reply that arrives late
  from the killed process is refused (`run_closed`): the run is no longer open.
- Nothing is deleted. The user message and what the agent already wrote stay.
  A stopped turn writes no failure card (`agent` message `failed`): the
  interruption is the user's, not the agent's.
- Approvals the turn already opened are NOT cancelled: they stay `pending` and
  expire on their own, as with any approval.
- Windows: the kill reaches the CLI process the seat started, not necessarily
  its children (`[NV]`: not verified with the real CLI, no tree kill).

`GET /v1/chat/bootstrap` lists the projects with their lead:

```json
{"ok":true,"projects":[{"id":"prj_01J…","name":"Demo","root_label":"demo",
  "status":"active","lead":{"agent_id":"ag_01J…","name":"Ernesto",
  "status":"idle","pane_id":"w1:p3"}}]}
```

`root_label` is the last element of the project directory, never the absolute
path. A project has exactly one lead and at most eight seats; the host has at
most six live panels.

**MCP tools.** The seats talk to the host over the internal MCP endpoint
(loopback only, one token per panel or per run). CORE7 adds four tools to the
CORE6B surface: `project.create` (name, optional `path` relative to
`workspaces_root`), `agent.spawn` (name, optional `role` and `project_id`),
`delegation.assign` (`to_agent_id`, `task`, optional `conversation_id`) and
`delegation.complete` (`delegation_id`, `result`). An invalid call is `422`
with `detail.code` and `detail.field`; a refused call is `409` with its typed
code. See [`errors.md`](errors.md).

### Chat progress (CHAT-CORE-PROGRESS)

`chat.progress` (`text`) shows the user, in one line, what Alfred is doing before
a long task. It is optional, does not close the turn and does not replace
`chat.reply`. `text` is 1 to 120 characters of plain text: control characters are
refused, runs of spaces collapse, and text outside those bounds is refused, never
cut (`422 invalid_arguments`, `field: "text"`). Clients show it as plain text.

- Answers `{"status":"accepted"}`, or `{"status":"throttled"}` when the line comes
  less than 5 seconds after the last accepted one of the same run. A throttled
  line is not an error: the stored line stays and nothing is journaled. The model
  must not repeat it.
- A call on a run that already closed is `409 run_closed`.
- Schema version 12 adds `run.progress` and `run.progress_at`. The line is
  cleared when the run closes (`completed`, `failed`, `interrupted`, or stopped by
  the user), and at a restart.

On the event stream, `chat.progress` carries `conversation_id`, `run_id` and
`text`: the accepted line, or `text: null` when the run closes. It is not `chat.activity`,
which CORE7 already uses for delegation cards. It is journaled like any chat frame
and replayed by `Last-Event-ID`. Clients that predate it ignore the event and the
`progress` field.

**SSE.** `chat.message` and `chat.conversation` travel on the existing
`GET /v1/events` with the existing framing, keepalive and revocation:
`chat.message` carries `conversation_id`, `message_id`, `seq`,
`change: created|updated`, `author_kind` and `preview` — never the full body.
When a run closes, the seat emits `change: updated` for the agent message with
`state: "complete"` (reply committed) or `state: "failed"` (failure card).
Each turn change also emits `change: updated` for the user message that opened
it, with `author_kind: "user"` and `turn_state` (`running` when the agent takes
it; `completed`, `failed` or `interrupted` when it closes, and `interrupted` for
a turn open at a restart). Those frames carry no `state`: clients that read
`state` on `updated` must ignore frames without it, as the agent message frames
above are the only ones that set it.
Every chat frame has an SSE `id` equal to the global chat `revision`. On
reconnect, `Last-Event-ID` replays the journal after that id before any live
frame; state frames carry no id and are not replayed. A malformed or negative
header is `400 invalid_cursor`. Replay is bounded (10 000 frames); a client that
exceeds it still has the timeline as the authoritative resync. See
[`state.md`](state.md).

### Approvals (CORE8)

An approval is an explicit request bound to **one concrete action**. It is
opened by the agent's own hook or plugin, it waits for a decision, it is used
once and it is audited. Only a signed device with the `APPROVE` grant decides:
a message, another agent or the agent itself never approves anything.

Two surfaces share the domain and never mix their credentials:

- **hook** (`X-HN-Hook-Token`, loopback only): `POST /v1/chat/approvals`,
  `GET /v1/chat/approvals/{id}` and `POST /v1/chat/approvals/{id}/consume`.
  These are not device routes: a device signature is never accepted there and
  the hook token is never accepted on the device routes.
- **device** (signed): `GET /v1/chat/approvals` (the inbox) and
  `POST /v1/chat/approvals/{id}/decision`.

The hook token is **per run** (`orchestrator-chat.md` §5.2): the host mints one
when it launches the run, hands it to the CLI in `HN_HOOK_TOKEN`, and revokes it
when the run closes. A resident panel has one for its seat instead, because it
serves many turns. The host, not the body, decides which run an approval
belongs to:

- `POST /v1/chat/approvals` stamps the run and the conversation of the token;
  a body that declares another `run_id` or `conversation_id` is `403
  hook_scope_mismatch`.
- `GET /{id}` and `POST /{id}/consume` only reach an approval of that run (or
  of that seat, for a panel); anything else is `403 hook_scope_mismatch` and the
  approval does not change.
- A token of a run that already closed is `403 hook_token_revoked`. The
  host-wide token of `/v1/hooks/report` is `403 hook_token_not_scoped` here: it
  names no run, so it can never open or burn an approval.

`POST /v1/chat/approvals` opens the approval:

```json
{"kind":"command","tool":"bash","summary":"Ejecutar comando: cargo test",
 "cwd":"D:\\proyectos\\demo","paths":[],
 "action_digest":"sha256:9f86d0…","payload_redacted":{"command":"cargo test"},
 "ttl_seconds":120}
```

`kind` is `command|network|edit|tool`; `action_digest` is `sha256:` plus 64
hexadecimal digits; `ttl_seconds` is 1…300 and defaults to 120. The host is
the authority of `expires_at`. The answer is `201`:

```json
{"ok":true,"approval_id":"apr_01J…","status":"pending","expires_at":"2026-10-22T10:12:00.000Z"}
```

`GET /v1/chat/approvals/{id}` answers only what the plugin needs — the state
and the deadline, never the payload:

```json
{"ok":true,"approval_id":"apr_01J…","status":"pending","expires_at":"2026-10-22T10:12:00.000Z"}
```

`status` is `pending|approved|denied|expired|consumed`. An approval whose TTL
passed is `expired` as soon as anybody looks at it.

A pending approval climbs its levels on the clock: the lead has 40 s to decide,
then the CTO has 40 s, then the operator has the rest of the TTL. A level with
nobody alive is skipped and the `reason` becomes `decider_unavailable`; a TTL
under 15 s goes straight to the operator. The approval carries `level`
(`lead|cto|user`, `null` when the policy decided), `escalated_from`, `reason`,
`decider_kind` and `decider_id`. The climb only goes up, so a level is never
asked again.

`POST /v1/chat/approvals/{id}/consume` burns the approval just before the
action runs. The body is `{}` or `{"action_digest":"sha256:…"}`; when the
digest is present it must be the one the approval was opened with. The answer
is `{"ok":true,"approval_id":"apr_01J…","status":"consumed"}`. A second
consumption is `409 approval_already_decided`: the action never runs twice.

`GET /v1/chat/approvals?status=&conversation_id=` is the inbox of the
operator. Without `status` it answers the pending ones; `status` accepts the
five states of the lifecycle. The answer is `{"ok":true,"approvals":[…]}`,
each row with `id`, `conversation_id`, `seat_id`, `run_id`, `pane_id`, `kind`,
`tool`, `summary`, `cwd`, `paths`, `risk`, `action_digest`, `status`, `scope`,
`expires_at`, `decided_by`, `decided_at`, `payload_redacted` and the `audit`
rows (`device_id`, `action_digest`, `decision`, `scope`, `decided_at`).

For the mail actions (`mail.send`, `mail.attach_path`) the row carries no
`payload_redacted`. A device with `APPROVE` gets `details` instead, the shape of
`mail.md` («Detalles de la aprobación»); a device with `READ_STATE` only, and any
agent, hook or lead token, gets the row without `details`. The same rule applies
to the decision answer: an `APPROVE` device sees `details`, the rest do not.

`POST /v1/chat/approvals/{id}/decision` is the operator's answer:

```json
{"decision":"approve","scope":"once","client_id":"8a1c…"}
```

`decision` is `approve` or `deny`; `scope` is `once` (the only scope of this
version). The **decider is the signed device** (`decided_by` and the audit
`device_id`), as in the mode settings; `client_id` is only the idempotency key.
The same `client_id` always gets the same answer back; any other request over
an approval that is already resolved is `409 approval_already_decided`. The
answer is `{"ok":true,"approval":{…}}` with the whole record.

**Exclusions (F2C1).** `exclude` is an optional list of `message_id`s the operator
leaves out of a mail batch (`mail.move`):

```json
{"decision":"approve","scope":"once","client_id":"8a1c…","exclude":["m2"]}
```

At most 20 entries of at most 512 characters; an empty list is the same as none,
and only a device with `APPROVE` may send it. Only `approve` takes exclusions, only
a `mail.move` approval takes them, and every id must be in the batch. A refusal
decides nothing: `422` with `code` `invalid_exclusion` (a denial, another tool, or
every message excluded), `422` with `unknown_message` (an id not in the batch), or
`422` with `invalid_body` and `field` `exclude` (too many entries or too long).
The excluded items stay in the approval, marked `"excluded": true`, and show in
its `details` for an `APPROVE` device; `mail.move_apply` skips them.

**Visibility (APK8).** Every pending approval is listed by
`GET /v1/chat/approvals?status=pending` and counted in
`pending_user_approvals`. An approval that belongs to a conversation also has
its timeline line `kind=approval`, born `pending` and rewritten in place by its
decision, keeping its `seq`. An approval with no conversation (a seat token
with no delegation) waits at the operator (`level=user`), because no agent
can answer it; it has **no timeline line** (there is no conversation to write
it in) and is seen only in the inbox and in `pending_user_approvals`.

The **digest is the identity of the action**: an action that was already asked
cannot be asked again, not even after a denial, an expiry or a consumption
(`409 approval_digest_reused`), so a replay never reaches a second execution
and the only way to retry is to build a different action. The only reuse that
is not an error is the same seat asking again while that approval is still
`pending`: it gets the same approval back. The **TTL** is 120 s
by default and 300 s at most; after it the approval is `expired` and the hook
denies.

**Approval mode (CORE9-D).** `GET /v1/chat/approvals/settings` (signed,
`READ_STATE`) answers `{"ok":true,"settings":{"global":"delegated","projects":[…]}}`.
Each project entry is `{"project_id","mode"}` and exists only as an override.
`POST /v1/chat/approvals/settings` (signed, **`APPROVE`**) takes
`{"client_id","mode","project_id"?}`, where `mode` is `delegated` or
`always_user`. Without `project_id` it sets the global mode; with one, the mode of
that project. The answer is `{"ok":true,"settings":{…},"replayed":false,"escalated":0}`,
where `escalated` counts the pending `lead` and `cto` approvals that the switch
sent to the operator. A `client_id` that was already used answers
`replayed:true` and changes nothing. A mode that is neither value is
`422 invalid_mode`; a project that does not exist is `422 unknown_project`.
`ADMIN` does not grant this route, and no agent surface reaches it. Every
accepted change is audited (from, to, scope) with the signing device as the
decider (`decider_id` = `device_id`); `client_id` is only the idempotency key.
It publishes `chat.approval_settings`. The `mode` key of `approvals.toml` is
ignored until CORE9-E; the mode lives in `chat.db`.

**MCP tool `approval.request`.** The agent whose tool call goes through MCP
opens the approval itself: `{"kind","tool","summary","args","cwd","paths",
"ttl_seconds"}`. The host computes the digest of the action, opens the
approval and the call **blocks** until the operator answers. The answer is
`{"approval_id":"apr_01J…","status":"approved"}`; a refusal is the typed error
`approval_denied` or `approval_expired`. The approval is consumed before the
tool returns, so the action behind it runs once.

**Hook.** `hn-uac-hook` is the blocking hook of the CLIs that support one
(Claude, Codex). It opens the approval, waits for the decision and answers with
the exit code the CLI expects:

| exit | meaning |
|------|---------|
| `0` | the action may run: the operator allowed it and the approval was consumed just before |
| `2` | the action is denied: the operator refused it, the approval expired, the host never gave an answer (timeout, refused connection, malformed response), **or** the binary itself was called wrong (an invalid argument) |

`2` covers every way of not having a permission, including a bad invocation,
because a CLI that reads any other code as "the hook broke" would let the action
run. There is no other exit code.

**Preflight.** Before a seat gets a panel, the host proves that the blocking
UAC adapter of its CLI works: it runs a canary that loads the plugin and tries
a test command, and it checks that the plugin loaded, that it blocked the
command *and* that the command left no trace on disk. A seat whose adapter
failed is `unsafe`: it is visible in `uac_state` and it is never launched.
Without a configured plugin the seats stay `unchecked` (degraded and visible).

The OpenCode canary costs no quota: it runs against an in-process provider that
speaks the OpenAI-compatible protocol and always asks for the test command, the
same one `tooling/opencode-uac-plugin/e2e/mock-provider.mjs` gives the `e2e/`
harness. OpenCode 1.19.3 bundles the provider SDK, so the `"npm"` field of the
canary provider selects it and installs nothing. The CLI does install its own
plugin SDK (`@opencode-ai/plugin`) in `<data>/uac-canary/xdg/config/opencode` on
the first plugin load; `tooling/seed-uac-canary.ps1` preinstalls that tree with
pnpm, so the first canary is offline and costs 2-3 s instead of a download.

## Estado para Alfred (`status.overview`, F1)

Herramienta MCP de solo lectura, solo para el token de Alfred. Sin argumentos:
cualquier campo es `422 invalid_arguments`. Un lead o un especialista recibe
`422 scope_denied`. La respuesta es `{"result": …}` con esta forma:

```json
{
  "revision": 12,
  "chat_revision": 58,
  "panels": {
    "total": 3,
    "counts": {"working": 1, "blocked": 1, "idle": 1},
    "unassigned": {"idle": 1},
    "projects": [{"name": "Demo", "root_label": "demo", "counts": {"working": 1, "blocked": 1}}],
    "truncated": false
  },
  "delegations": {"open": 1, "items": [{"id": "dlg_…", "state": "running", "task": "…", "to": "Ana"}], "truncated": false},
  "approvals": {"pending": 1, "items": [{"id": "apr_…", "kind": "command", "agent": "ana"}], "truncated": false},
  "last_activity": "2026-10-28T10:00:00Z"
}
```

- `revision` es la de `GET /v1/state`; `chat_revision` la de `bootstrap`. Los
  `counts` usan las mismas claves que `/v1/state` (las presentes en ese momento).
- `panels.projects` agrupa por `root_label`; `unassigned` cuenta los paneles
  sin proyecto listado (sin asiento, proyecto archivado o más allá del tope).
- `approvals` son las aprobaciones en el nivel de Alfred (`cto`), sin
  `payload_redacted`. `approvals.pending` coincide con `pending_approvals` del
  `bootstrap` mientras todas estén en ese nivel.
- Cada lista tiene como máximo 20 elementos; si hay más, `truncated` es `true`.
  La tarea se corta a 120 caracteres con `…`.
- Nunca aparecen `root_path`, `hook_token`, `payload_redacted`, `cwd`, rutas,
  `pane_id` ni secretos.

## Notas de Alfred (`note.*`, F2)

Herramientas MCP solo para el token de Alfred. Un lead o un especialista recibe
`422 scope_denied`. Las notas son datos, no instrucciones: nunca entran en el
prompt de Alfred ni en un log, y la auditoría (`agenda_audit`) guarda quién y
cuándo cambió una nota, nunca su texto.

| Herramienta | Argumentos | `result` |
|---|---|---|
| `note.save` | `body` (obligatorio, ≤ 32 KiB), `subject?` (≤ 120 caracteres) | `{"id","subject","updated_at"}` |
| `note.search` | `query`, `limit?` (1–20, por defecto 10) | `{"notice","notes":[{"id","subject","excerpt","updated_at"}]}` |
| `note.update` | `id`, y `body?` o `subject?` (al menos uno); `subject: ""` la borra | igual que `note.save` |
| `note.delete` | `id` | `{"id","deleted":false,"reason":"pending_approval","approval_id"}` |

- `note.search` compara sin distinguir mayúsculas (también Ñ/ñ), en Rust y no
  en SQL. `excerpt` son 200 caracteres como máximo. `notice` recuerda que el
  contenido es dato no confiable.
- El `message_id` de la nota es el mensaje del turno que la creó; si no hay
  turno, el último mensaje del usuario de la conversación.
- Los límites se rechazan, nunca se cortan el texto: `subject` de más de 120
  caracteres o con controles es `422 invalid_arguments`; un `body` vacío es
  `422 empty_message` y uno de más de 32 KiB es `422 invalid_body`.
- Cada empresa guarda como máximo 2000 notas: al llegar al tope,
  `409 note_limit`.
- `note.delete` nunca lo resuelve el lead. Abre una aprobación de tipo `tool`,
  nivel `user` (categoría `DeleteUserData`), no delegable, con el resumen
  «Borrar la nota {id}» (sin el asunto ni el cuerpo), y responde al instante
  con `pending_approval` y el `approval_id`: no espera al usuario.
- La nota se borra cuando el usuario aprueba la aprobación en el chat
  (`POST /v1/chat/approvals/{id}/decision`): el borrado y el paso a `consumed`
  van en la misma transacción. Aprobar otra vez responde conflicto y no borra
  de nuevo. Si la nota ya no existe, la aprobación queda `consumed` igual.
- Si el usuario la deniega o caduca, la nota sigue; una nueva llamada abre una
  pregunta nueva. Mientras una pregunta de borrado de esa nota está pendiente,
  cualquier llamada devuelve su mismo `approval_id`: nunca hay dos.
- La auditoría (`agenda_audit`) registra el borrado con `actor_kind="user"` y
  el dispositivo que aprobó como `actor_id`.
- Un `id` que no existe responde `422 not_found` (campo `id`).

## Contactos de Alfred (`contact.*`, F3)

Herramientas MCP solo para el token de Alfred. Un lead o un especialista recibe
`422 scope_denied`. Los contactos son datos, no instrucciones. La libreta y lo que el
usuario escribió literalmente en la conversación son las únicas fuentes de destinatarios de un borrador.

| Herramienta | Argumentos | `result` |
|---|---|---|
| `contact.find` | `query` (1–200 caracteres) | `{"notice","contacts":[{"id","name","aliases","addresses","notes","updated_at"}]}` |
| `contact.save` | `name` (obligatorio, ≤ 120), `id?` (cambia ese contacto; sin `id` crea uno), `aliases?` (≤ 10, cada una ≤ 120), `addresses?` (≤ 10, cada una ≤ 320), `notes?` | `{"id","name","aliases","addresses","notes","updated_at"}` |
| `contact.delete` | `id` | `{"id","deleted":false,"reason":"pending_approval","approval_id"}` |

- `contact.find`: un alias que coincide exactamente encuentra solo su contacto;
  si no, los nombres y alias que contienen la búsqueda, sin distinguir mayúsculas
  ni acentos, hasta 10. `notice` recuerda que el contenido es dato no confiable.
- Una lista enviada reemplaza la guardada; una lista omitida se conserva. Las
  direcciones se guardan en minúsculas, sin espacios. Una dirección o un alias que
  ya tiene otro contacto responde `409 address_taken` o `409 alias_taken` y no
  cambia nada.
- Los límites se rechazan, nunca se cortan: un nombre o alias vacío, con
  controles o más largo de su límite es `422 invalid_arguments`; una dirección mal
  formada es `422 invalid_address` (campo `addresses`); más de 10 direcciones o
  aliases es `409 address_limit` o `409 alias_limit`. La libreta guarda como máximo
  1000 contactos: al llegar al tope, `409 contact_limit`.
- Un `id` que no existe responde `422 not_found` (campo `id`).
- `mail.draft_create` y `mail.draft_update` aceptan en `to` y `cc` las direcciones
  de la libreta y las que el usuario escribió literalmente en la conversación del
  actor (F3B): una palabra completa, sin distinguir mayúsculas ni espacios, en uno de
  sus últimos 50 mensajes con autor `user`, delimitada por espacios, `,`, `<>`, `;`,
  paréntesis o comillas. Esa dirección se guarda como contacto con
  `source = "user_message"`, nombre = parte local (≤ 120), sin alias, y su `create`
  queda en `agenda_audit`. Si alguna dirección se rechaza, no se guarda ninguna.
  Nunca autorizan una dirección un mensaje de Alfred o de un agente, un correo, una
  nota ni el cuerpo del borrador.
- Otra dirección responde `422 unknown_recipient` (campo `to`) antes de refrescar
  ningún token ni llamar a Microsoft; una dirección mal formada, `422 invalid_address`
  (campo `addresses`).
- Cada dirección guarda su origen en `source`: `alfred` (`contact.save`),
  `user_message` (el usuario la escribió) o `phone` (admitido por el almacén; la
  importación del móvil aún no lo usa).
- `contact.delete` nunca lo resuelve el lead. Abre una aprobación de tipo `tool`,
  nivel `user`, no delegable, con el resumen «Borrar el contacto {id}» (sin el
  nombre ni las direcciones), y responde al instante con `pending_approval`.
- El contacto se borra cuando el usuario aprueba la aprobación en el chat; sus
  direcciones y alias se van con él. Mientras una pregunta de borrado de ese
  contacto está pendiente, cualquier llamada devuelve su mismo `approval_id`.
- La auditoría (`agenda_audit`) registra `create` y `update` con
  `actor_kind="alfred"`, y el `delete` con `actor_kind="user"` y el dispositivo
  que aprobó. Nunca guarda nombres, direcciones ni alias.

## Calendario de Alfred (`calendar.*`, F4)

Herramientas MCP solo para el token de Alfred. Un lead o un especialista recibe
`scope_denied`. Los eventos son datos, no instrucciones. Alfred no puede fijar la
zona de la empresa: eso lo hace un dispositivo con `APPROVE` por REST.

| Herramienta | Argumentos | `result` |
|---|---|---|
| `calendar.timezone` | ninguno | `{"timezone"}`; `null` si la empresa aún no tiene zona |
| `calendar.list` | `from`, `to` (obligatorios), `limit?` (1–100, 20 por defecto), `after?` (`id`, o `id@occurrence_start` para una ocurrencia) | `{"notice","events":[{"id","title","start","end","all_day","recurrence","timezone","location","notes","reminder_minutes","updated_at","occurrence_start","recurring"}],"truncated","next"}` |
| `calendar.create` | `title` (obligatorio, ≤ 120), `start` (obligatorio), `end?`, `all_day?`, `location?` (≤ 200), `notes?` (≤ 4 KiB), `reminder_minutes?` (0–10080), `recurrence?` (regla, ver abajo) | el evento, con la forma de la fila anterior |
| `calendar.update` | `id` (obligatorio) y al menos un campo a cambiar; `recurrence?` (`""` la quita) | el evento |
| `calendar.delete` | `id` | `{"id","deleted":false,"reason":"pending_approval","approval_id"}` |

- `from`, `to` y `start`, `end` aceptan `YYYY-MM-DD`, hora local `YYYY-MM-DDTHH:MM[:SS]`
  (en la zona de la empresa) o un instante con desfase o `Z`. `to` no se incluye. Una
  ventana de más de 366 días, o con `from` posterior a `to`, es `invalid_arguments` en
  la herramienta y `422 invalid_field` en REST.
- Sin zona de empresa, Alfred no crea nada: `timezone_unset` (campo `timezone`) y
  pregunta al usuario. Ningún evento queda sin zona.
- Un evento de día completo toma fechas; si no hay `end`, dura hasta el día siguiente.
  Una hora local que el cambio de hora salta es `invalid_local_time`; una que se
  repite toma la primera de las dos.
- `calendar.update`: `"end": ""` quita el fin, `"reminder_minutes": -1` quita el aviso y
  `"recurrence": ""` deja el evento en una sola vez. Sin ningún campo que cambiar es
  `invalid_arguments`.
- `calendar.delete` nunca lo resuelve el lead. Abre una aprobación de tipo `tool`, nivel
  `user`, con el resumen «Borrar el evento {id}», y responde al instante con
  `pending_approval`. El evento se borra cuando el usuario aprueba en el chat; si lo
  rechaza, se queda. Mientras la pregunta esté pendiente, cualquier llamada devuelve
  su mismo `approval_id`.
- Los tiempos de la respuesta de Alfred van en UTC y se llaman `start` y `end`. La
  API REST devuelve los mismos campos como `start_utc` y `end_utc` (ver abajo): la
  diferencia de nombre es deliberada hasta que se unifiquen.
- Recordatorios: al llegar `reminder_minutes` antes del inicio, se crea una tarjeta
  `calendar_reminder` en la conversación del evento (o en la más reciente no archivada)
  y el evento queda marcado con `reminded_at`. Un aviso con más de 24 h de retraso se
  marca sin tarjeta. No hay notificación push.
- Límites: 5000 eventos por empresa (`calendar_limit`).

Recurrencia (CALENDARIO-F4b):

- Regla: subconjunto de RFC 5545, con o sin prefijo `RRULE:`. `FREQ` (DAILY, WEEKLY,
  MONTHLY o YEARLY, obligatorio), `INTERVAL` (1–999), `BYDAY` (MO a SU), y `COUNT` (1–1000)
  o `UNTIL` (fecha o instante). La forma se normaliza al guardarla, p. ej.
  `FREQ=WEEKLY;INTERVAL=1;BYDAY=MO`.
- Errores: una regla fuera del subconjunto (p. ej. `FREQ=HOURLY`) es
  `unsupported_recurrence`; una regla mal escrita es `invalid_recurrence`. Un `UNTIL`
  anterior a la primera ocurrencia es `invalid_arguments` con campo `recurrence` (REST:
  `422 invalid_field`). Ninguno crea ni cambia nada.
- Horas: cada ocurrencia se calcula en la zona del evento. Una hora que el cambio de
  hora salta toma el primer instante válido después; una repetida, la primera. Un día
  completo conserva su duración de pared.
- Meses cortos: `MONTHLY` y `YEARLY` saltan las fechas que no existen (el 31 de febrero).
  `COUNT` cuenta solo las ocurrencias que existen.
- Listas: un evento de serie sale una vez por ocurrencia, con `recurring: true` y su
  `occurrence_start`. Un evento sin regla sale con `recurring: false` y su `occurrence_start`
  igual a `start`. `truncated: true` significa que la página está llena o que se agotó el
  presupuesto de cálculo (10 000 pasos por consulta); en ese caso hay que pedir la página
  siguiente con `after` igual a `next`. Si `truncated` es `true` y la lista viene vacía,
  acota la ventana.
- Edición y borrado: siempre sobre la serie entera. Cambiar la regla, el inicio o el aviso
  vuelve a armar el aviso.
- Avisos: uno por ocurrencia. La tarjeta `calendar_reminder` lleva la `occurrence_start`
  de su ocurrencia. Igual que en un evento suelto, un aviso con más de 24 h de retraso se
  marca sin tarjeta.
- `/v1/state`: `calendar_next` puede ser la siguiente ocurrencia de una serie, y
  `calendar_today` cuenta las ocurrencias del día de la zona de la empresa.
- Fuera de F4b: `.ics` (import y export) pasa a F4c, y las ocurrencias sueltas (editar o
  borrar una sola) no existen todavía.

## Agenda de la app (AGENDA-REST): `/v1/notes` y `/v1/contacts`

Las notas y los contactos que la app guarda y lee. Los permisos están en
`auth.md` (matriz). Los límites son los mismos que los de Alfred (`note.*` y
`contact.*`), porque cada cuerpo pasa por el parser de su herramienta. Todas las
rutas van firmadas por el dispositivo.

Una escritura deja una línea en `agenda_audit` con el dispositivo que la hizo, y
nunca guarda el título, el cuerpo, el nombre ni una dirección.

### Notas

- `GET /v1/notes?q=&before=&limit=` (`READ_STATE`). Las más recientes primero
  (`updated_at`, luego `id`). `q` busca en el asunto y el cuerpo sin distinguir
  mayúsculas. `before` es el `id` de la última nota de la página anterior.
  `limit` va de 1 a 100 y vale 20 por defecto. `excerpt` es el cuerpo en una
  línea, recortado a 200 caracteres.

```json
{ "ok": true, "notes": [
  { "id": "nt_01JAB4...", "subject": "Cliente", "excerpt": "presupuesto 1.250 €",
    "updated_at": "2026-10-29T09:00:00.000Z" } ],
  "next": "nt_01JAB4..." }
```

`next` es el `id` de la última nota cuando la página está llena, y `null` en la
última página.

- `GET /v1/notes/{id}` (`READ_STATE`). `{ok, note}`; `404 not_found` si no existe.
- `POST /v1/notes` (`RESPOND`). Cuerpo `{"title"?, "body", "message_id"?}`. Responde
  `201 {ok, note}`. `title` es el asunto (una línea, hasta 120 caracteres; si pasa,
  `422 invalid_arguments` en `subject`). `body` es obligatorio: vacío tras recortar
  es `422 empty_message`; más de 32 KiB es `422 invalid_body`; ambos en `body`. Un
  `message_id` debe existir, o llega `422 unknown_message` en `message_id`. Un
  campo que no existe, o de otro tipo, es `422 invalid_body`.
- `POST /v1/notes/{id}/update` (`RESPOND`). Cuerpo `{"title"?, "body"?}`. Un campo
  ausente conserva su valor; `"title": ""` quita el asunto. Responde `200 {ok, note}`.
- `POST /v1/notes/{id}/delete` (`APPROVE`). Se borra al momento: la acción es del
  usuario. Responde `200 {ok, id}`; si la nota ya no existe, `404 not_found`.

`409 note_limit` cuando la empresa ya tiene 2000 notas.

### Contactos

Un contacto es `{id, name, aliases, addresses, notes, created_at, updated_at}`.
Cada dirección va como `{"address", "source"}`, con `source` igual a `app` (la
guardó la app), `phone` (vino de la importación), `alfred` o `user_message`.

- `GET /v1/contacts?q=&after=&limit=` (`READ_STATE`). Igual que las notas: `q`
  busca en el nombre y en los alias, `after` es el `id` de la última fila de la
  página anterior. Responde `{ok, contacts, next}`.
- `GET /v1/contacts/{id}` (`READ_STATE`). `{ok, contact}`; `404 not_found` si no existe.
- `POST /v1/contacts` (`RESPOND`). Cuerpo `{"name", "aliases"?, "addresses"?, "notes"?}`.
  `name` es obligatorio (hasta 120 caracteres). Hasta 10 alias y 10 direcciones; cada
  dirección es `local@dominio.tld`, sin espacios, y se guarda en minúsculas. Responde
  `201 {ok, contact}`.
- `POST /v1/contacts/{id}/update` (`RESPOND`). Mismo cuerpo. Un campo ausente conserva
  su valor; una lista presente reemplaza a la anterior. Responde `200 {ok, contact}`.
- `POST /v1/contacts/{id}/delete` (`APPROVE`). Borra el contacto con sus direcciones y
  alias. Responde `200 {ok, id}`.
- `POST /v1/contacts/import` (`APPROVE`). Ver la sección siguiente.

Errores de contactos: `422 invalid_arguments` si una lista pasa de 10 elementos o un
texto pasa de su límite (campo `addresses`, `aliases` o `name`); `422 invalid_address`
(campo `addresses`) si una dirección no tiene la forma `local@dominio.tld`;
`409 address_taken` (el mensaje nombra al contacto dueño); `409 alias_taken`;
`409 contact_limit` (1000 contactos). Un contacto que ya no existe responde
`404 not_found`.

### Importar la agenda del móvil

`POST /v1/contacts/import` con `{"contacts": [{"name", "addresses": [...]}, ...]}`.
Admite hasta 500 entradas; más responde `422 invalid_field` en `contacts`
(`import_limit`). Cada entrada tiene su propia transacción:

- Si el nombre coincide, sin distinguir mayúsculas ni tildes, con un contacto
  existente, se añaden sus direcciones nuevas y nada más. Cuenta como `merged`.
- Si no coincide con ninguno, se crea el contacto con sus direcciones (`source`
  `phone`). Cuenta como `created`.
- Si una dirección ya la tiene otro contacto, la entrada se omite sin escribir nada.
  Cuenta como `skipped`. Nunca se sobrescribe un nombre, un alias ni una nota.
- Una entrada sin direcciones, con una dirección inválida, o que superaría un
  límite, se rechaza y no escribe nada. Va a `rejected` con su posición y su código.
  Las entradas ya escritas se quedan: cada una tiene su propia transacción.

```json
{ "ok": true, "created": 2, "merged": 1, "skipped": 1,
  "rejected": [ { "index": 3, "code": "no_address" },
                { "index": 4, "code": "invalid_address" } ] }
```

Los códigos de `rejected` son `no_address`, `invalid_address`, `address_limit`,
`contact_limit` e `invalid_arguments` (un nombre que el parser no admite). El
informe nunca repite un nombre ni una dirección: la entrada se identifica solo por
su posición. Un fallo que no sea de esos códigos corta la importación con el error
de la tabla general.

## Calendario de la app (CALENDARIO-F4): `/v1/calendar` y `/v1/company/timezone`

Un evento es `{id, title, start_utc, end_utc, all_day, recurrence, timezone, location,
notes, reminder_minutes, reminded_at, reminded_occurrence, origin_kind, origin_id,
conversation_id, created_at, updated_at, uid}`, con los instantes en UTC
(`YYYY-MM-DDTHH:MM:SS.mmmZ`). `recurrence` es la regla normalizada o `null`; en una
entrada de lista, `start_utc` es la ocurrencia (ver «Recurrencia» en la sección de Alfred). Cada cuerpo pasa
por el parser de la herramienta `calendar.*`, así que sus límites y códigos son los
de Alfred. Los permisos están en `auth.md`.

Una escritura deja una línea en `agenda_audit` con el dispositivo que la hizo, sin
título, notas ni ubicación.

- `GET /v1/calendar/events?from=&to=&after=&limit=` (`READ_STATE`). `from` y `to` como
  en la herramienta; `to` no se incluye. Ventana máxima de 366 días. `limit` de 1 a
  100, 20 por defecto. Responde `{ok, events, truncated, next}`. Cada entrada es un
  evento con `occurrence_start` y `recurring` (ver «Recurrencia»). `truncated` es `true`
  si la página está llena o se agotó el cálculo; `next` es el cursor de la última
  entrada (`id` o `id@occurrence_start`) cuando `truncated` es `true`, y `null` si no.
  Un `after` que no nombra nada es `422 invalid_field` (campo `after`). Sin zona de
  empresa, `409 timezone_unset`.
- `GET /v1/calendar/events/{id}` (`READ_STATE`). `{ok, event}`; `404 not_found` si no existe.
- `POST /v1/calendar/events` (`RESPOND`). Cuerpo `{"title", "start", "end"?, "all_day"?,
  "location"?, "notes"?, "reminder_minutes"?, "recurrence"?}`. Responde `201 {ok, event}`.
  Sin zona, `409 timezone_unset`; con 5000 eventos, `409 calendar_limit`; una regla no
  soportada, `422 unsupported_recurrence`, y una mal escrita, `422 invalid_recurrence`.
- `POST /v1/calendar/events/{id}/update` (`RESPOND`). Mismos campos, todos opcionales;
  `"end": ""` quita el fin, `"reminder_minutes": -1` quita el aviso y `"recurrence": ""`
  quita la regla. Responde `200 {ok, event}`.
- `POST /v1/calendar/events/{id}/delete` (`APPROVE`). Se borra al momento: la acción es
  del usuario. Responde `200 {ok, id}`; si el evento ya no existe, `404 not_found`.
- `GET /v1/company/timezone` (`READ_STATE`). `{ok, timezone}`; `timezone` es `null`
  hasta que alguien la fija.
- `POST /v1/company/timezone` (`APPROVE`). Cuerpo `{"timezone": "Europe/Madrid"}`, un
  nombre IANA. Responde `200 {ok, timezone}`. Un nombre que no existe es `422
  invalid_field` (campo `timezone`); un campo extra, `422 invalid_body`. La zona se
  fija una vez: un segundo intento responde `409 timezone_already_set` y la zona no
  cambia. La fijación queda en `agenda_audit`.

`/v1/state` suma, sin quitar nada, `calendar_today` (cuántos eventos empiezan hoy en la
zona de la empresa), `calendar_next` (`{id, start_utc}` del siguiente, o `null`; sin
título) y `reminders_due` (avisos pendientes de entregar). Ninguno lleva títulos.

## Calendario en `.ics` (CALENDARIO-F4c): `/v1/calendar/export.ics` e `/v1/calendar/import`

Un archivo `.ics` (RFC 5545, un subconjunto) sale y entra en el calendario de la empresa.
No hay sincronización: cada importación es un archivo que el usuario sube. El lector y el
escritor son propios, sin dependencias nuevas, y acotan todo lo que llega.

- `GET /v1/calendar/export.ics?from=&to=` (`READ_STATE`, presupuesto de lectura). `from` y
  `to` como en `GET /v1/calendar/events`: `to` no se incluye y debe ser posterior a `from`
  (`422 invalid_field`, campo `to`). Sin ellos, exporta todo. Responde `200` con
  `Content-Type: text/calendar; charset=utf-8` y
  `Content-Disposition: attachment; filename="excubia.ics"`. Más de 2000 eventos en la
  ventana: `422 too_many_events`; hay que acotarla. Sin zona de empresa, `409 timezone_unset`.
- `POST /v1/calendar/import` (`RESPOND`, presupuesto de escritura). El cuerpo es el archivo
  tal cual, con `Content-Type: text/calendar`; hasta 512 KiB (`413` si pesa más). Más de 500
  `VEVENT`: `422 too_many_events`. Lo que no es un calendario: `400 invalid_ics`. Sin zona de
  empresa, `409 timezone_unset`, aunque el archivo traiga horas con zona. Responde `200`:

```json
{"ok": true, "imported": 2, "skipped_existing": 0, "skipped_unsupported": 0, "details": []}
```

`index` es la posición del `VEVENT` en el archivo, desde 0. `details` nombra como mucho 50
omitidos, `exists` incluidos; los demás solo se cuentan. `skipped_unsupported` cuenta todo
lo omitido que no es `exists`, así que un archivo con un evento repetido y otro de serie
con `EXDATE` da `skipped_existing: 1`, `skipped_unsupported: 1`. Motivos: `exists`, `unsupported_recurrence`, `exception_instance`, `no_start`,
`invalid_time`, `invalid_text`, `invalid_recurrence`, `calendar_limit`. Un evento que
falla no para a los demás.

Un evento importado guarda su `UID` en `uid`; un `UID` que la empresa ya tiene es
`exists`, así que reimportar el mismo archivo no duplica nada. Un evento de la app se
exporta con `UID: <id>@excubia`, y reimportado en la misma empresa también es `exists`.
Sin `UID`, se hace uno desde el contenido (`imp-` y 32 hex de SHA-256 sobre el resumen, el
inicio, el fin, la duración y la regla): el mismo contenido da el mismo `uid`.

| Se importa | Se omite (motivo) |
|---|---|
| `DTSTART` en UTC, con `TZID` o flotante (hora de la zona de la empresa) o `VALUE=DATE` | Sin `DTSTART`: `no_start`; hora que no existe en la zona: `invalid_time` |
| `DTEND` o `DURATION`; `SUMMARY`, `LOCATION`, `DESCRIPTION` | `RECURRENCE-ID` (una ocurrencia cambiada): `exception_instance` |
| `RRULE` con `FREQ`, `INTERVAL`, `BYDAY`, `COUNT`, `UNTIL` | `EXDATE`, `RDATE`, otra parte de `RRULE`: `unsupported_recurrence`; mal escrita: `invalid_recurrence` |
| El primer `VALARM` con un `TRIGGER` de minutos antes del inicio (`-PT15M`) | Un `TRIGGER` que no es de minutos: sin aviso |
| Propiedades `X-` y de terceros, `VTIMEZONE`, otros componentes | Se ignoran sin omitir el evento |

La exportación escribe `UID`, `DTSTAMP`, el inicio y el fin (`VALUE=DATE` en los eventos
de día completo, con el fin exclusivo del RFC), `SUMMARY`, `LOCATION`, `DESCRIPTION`, `RRULE`
y un `VALARM` de `-PT<n>M` con `DESCRIPTION`. Las horas son UTC.

Ejemplo de un archivo que el calendario acepta:

```
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Excubia//Calendario//ES
BEGIN:VEVENT
UID:reunion-1@ejemplo.es
DTSTAMP:20261029T080000Z
DTSTART;TZID=Europe/Madrid:20261103T093000
DTEND;TZID=Europe/Madrid:20261103T103000
SUMMARY:Reunión de equipo
LOCATION:Sala 2\, planta baja
BEGIN:VALARM
ACTION:DISPLAY
DESCRIPTION:Reunión de equipo
TRIGGER:-PT15M
END:VALARM
END:VEVENT
BEGIN:VEVENT
UID:festivo-1@ejemplo.es
DTSTART;VALUE=DATE:20261105
DTEND;VALUE=DATE:20261106
SUMMARY:Festivo local
END:VEVENT
END:VCALENDAR
```

Los permisos están en `auth.md`.

## Reglas de aviso (ALERTAS-F5): `/v1/alerts` y el frame `alert`

Una regla dice qué cambio avisa al móvil. Se guardan en la tabla `alert_rule` (migración 21).
El modo vive en `meta` (`alerts_mode`); sin valor, es `all`.

**Modos**
- `all` (por defecto): un agente que se bloquea o termina avisa como siempre. Una regla
  `notify` que casa lo asocia a su `rule_id`.
- `rules_only`: solo avisa lo que casa con una regla `notify`.
- Una regla `mute` que casa silencia el cambio en cualquier modo y gana a una `notify`.

**Recordatorios de calendario**: la tarjeta siempre llega al chat. El frame `alert` solo sale
si una regla `notify` casa y ninguna `mute` lo impide.

**Frenos** (`crates/host/src/alert_policy.rs`): un cambio de estado de agente pasa tres frenos,
en orden. Solo `blocked` y `done` pueden avisar. Un cambio que vino de una instantánea (arranque
o recuperación) o de un agente retirado nunca avisa. Enfriamiento: como mucho un aviso por panel
cada 120 s; un `blocked` sin atender se repite a los 900 s. Presupuesto: 6 por agente y hora, 60
globales por hora y 400 por día. Horas de silencio: no hay por defecto; si se configuran, `done`
se aplaza a la hora de fin y `blocked` las ignora. Solo `ALLOW` publica el frame; `DEFER` y
`SUPPRESS` no. Una regla no salta los frenos: `mute` no publica nada, y `notify` publica si los
frenos lo dejan pasar. Los recordatorios de calendario no pasan por los frenos.

**Campos de una regla**: `id`; `trigger` (`pane_status` o `calendar_reminder`); `agent` (nombre
del agente, o `null` para cualquiera; solo en `pane_status`); `status` (`blocked` o `done`,
obligatorio en `pane_status` y prohibido en `calendar_reminder`); `action` (`notify` o `mute`);
`enabled`; `created_by` (`user` o `alfred`); `created_at`. Como máximo 100 reglas.

| Método | Ruta | Auth | Permiso | Propósito |
|--------|------|------|---------|-----------|
| `GET` | `/v1/alerts/rules` | firmada | `READ_STATE` | `{ok, rules}` |
| `POST` | `/v1/alerts/rules` | firmada | `RESPOND` | Crea una regla: `201 {ok, rule}` |
| `POST` | `/v1/alerts/rules/{rule_id}/update` | firmada | `RESPOND` | Cuerpo `{"enabled": bool}`; `{ok, rule}` |
| `POST` | `/v1/alerts/rules/{rule_id}/delete` | firmada | `RESPOND` | `{ok, id}` |
| `GET` | `/v1/alerts/mode` | firmada | `READ_STATE` | `{ok, mode}` |
| `POST` | `/v1/alerts/mode` | firmada | `RESPOND` | Cuerpo `{"mode": "all" \| "rules_only"}`; `{ok, mode}` |

Ejemplo: `POST /v1/alerts/rules` con `{"trigger": "pane_status", "agent": "Bob", "status": "done",
"action": "notify"}` responde `201` con `{"ok": true, "rule": {"id": "ar_…", "trigger":
"pane_status", "agent": "Bob", "status": "done", "action": "notify", "enabled": true,
"created_by": "user", "created_at": "…"}}`.

Errores: `422 invalid_field` con el campo en `detail.field`; `409` con `detail` = `rule_exists`
(idéntica a otra) o `rule_limit` (100 reglas); `404` con `detail` = `not_found` al cambiar o
borrar un id que no existe; `403` sin `RESPOND` en las escrituras.

**Frame `alert`** (SSE, `event: alert`). Su cuerpo es `{"kind": "agent_blocked" | "agent_done" |
"calendar_reminder", "rule_id": "ar_…" | null, "ts": "…"}`. No lleva nombre de agente, título ni
texto: el móvil muestra un aviso genérico y lee el detalle del estado o del chat.

**`/v1/state`** suma `alert_rules: {"mode", "enabled_count"}`, sin quitar nada. Nunca lista reglas.

**Herramientas de Alfred** (`alert.*`, F5). Sin aprobación: una regla es reversible y solo cambia
qué avisa. Un lead recibe `scope_denied`.

| Herramienta | Argumentos | Resultado |
|-------------|------------|-----------|
| `alert.rule_create` | `trigger`, `agent?`, `status?`, `action` | `{"rule"}` |
| `alert.rule_list` | ninguno | `{"mode", "rules"}` |
| `alert.rule_update` | `id`, `enabled` (boolean) | `{"rule"}` |
| `alert.rule_delete` | `id` | `{"id"}` |
| `alert.mode_set` | `mode` (`all` o `rules_only`) | `{"mode"}` |

## Envío de correo (MAIL2A, `/v1/mail`)

Solo el operador envía. Flujo, reglas y códigos en `mail.md`, sección «Envío».

| Ruta | Permiso | Respuesta |
|---|---|---|
| `GET /v1/mail/accounts` | `ADMIN` | `200 {"ok","accounts":[{label,tenant,upn,send_enabled}]}`; sin tokens |
| `POST /v1/mail/drafts/{draft_id}/send` `{account, approval_id?}` | `APPROVE` | `202 {"ok","state":"awaiting_approval","approval_id","expires_in"}` sin `approval_id`; `202 {"ok","state":"pending","send_id","window_seconds"}` con él |
| `GET /v1/mail/drafts/{draft_id}?account=` | `APPROVE` | `200 {"ok","draft":{account,draft_id,from,to,cc,subject,body,content_type,attachments:[{name,size,sha256}]}}`, solo lectura; `403` para agentes y seats |
| `POST /v1/mail/drafts/{draft_id}/update` `{account, to?, cc?, subject?, body?}` | `APPROVE` | `200 {"ok","draft":{...}}` con la forma de la revisión; `404 mail_unknown_draft` si el host no creó el borrador; `400 mail_invalid_input` fuera de límites. Audita `draft_edit` con el dispositivo |
| `POST /v1/mail/sends/{send_id}/cancel` | `APPROVE` | `200 {"ok","send_id","state"}` |
| `GET /v1/mail/sends/{send_id}` | `APPROVE` | `200 {"ok","send_id","state"}`: `pending`, `sent`, `cancelled` o `failed`. Un `failed` añade `failure:{code,retry}` (tabla en `mail.md`); un `sent` no lo lleva |
| `POST /v1/mail/send/enable` `{account}` | `ADMIN` | `200`, la misma forma que `login/start` |
| `GET /v1/mail/send/enable/status?account=` | `ADMIN` | `200 {"ok","login"}` |
| `POST /v1/mail/send/disable` `{account}` | `ADMIN` | `200 {"ok","account","send_enabled":false}` |

Un `429` con `Retry-After` llega de `send` cuando se alcanza el límite por hora.

Ejemplo de un envío que Microsoft suspendió. El `failure` no lleva texto del
servidor: solo la causa en palabras fijas y la regla de reintento.

```json
{"ok": true, "send_id": "01J…", "state": "failed",
 "failure": {"code": "account_suspended", "retry": "no"}}
```

## Request bodies and signatures

The body used for the signature hash is the **exact received bytes**, not a
re-serialised model. Consequently the server does not require
`Content-Type: application/json`; the bytes are parsed directly as JSON by the
signed-body parser. A correctly signed body with a missing or wrong content type
is still valid.

The signed-body cap is 256 KiB, raised on the image route only to fit a base64
picture (`errors.md`).
