# Contract: HTTP surface

The host binds loopback by default. Reaching it from a phone is the transport's
job, not this process's: over Tailscale, or over the LAN listener below. Authoritative
code: `gateway/src/herdr_notify/api/app.py`, `.../api/actions.py`.

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
| `GET` | `/v1/state` | signed | `READ_STATE` | read | The full state read model |
| `GET` | `/v1/events` | signed | `READ_STATE` | read | SSE stream (see `state.md`) |
| `GET` | `/v1/catalog` | signed | `READ_STATE` | read | The agents this host can start, by name |
| `GET` | `/v1/peers` | signed | `READ_STATE` | read | Tailnet peers + host addresses to probe |
| `GET` | `/v1/devices` | signed | `ADMIN` | read | List paired devices ("My devices") |
| `POST` | `/v1/devices/{device_id}/revoke` | signed | `ADMIN` | write | Revoke one device |
| `POST` | `/v1/agents/{pane_id}/respond` | signed | `RESPOND` | write | Submit a prompt |
| `POST` | `/v1/agents/{pane_id}/keys` | signed | `SEND_KEY` | write | Send allowlisted keys |
| `POST` | `/v1/agents/{pane_id}/stop` | signed | `RESPOND` | write | Stop a live agent (the pane leaves the list) |
| `POST` | `/v1/agents/{pane_id}/focus` | signed | `RESPOND` | write | Focus the pane |
| `POST` | `/v1/agents/{pane_id}/seen` | signed | `READ_STATE` | write | Clear the "done" badge |
| `POST` | `/v1/agents/{pane_id}/read` | signed | `READ_STATE` | **write** | Read recent pane output |
| `POST` | `/v1/agents/{pane_id}/image` | signed | `RESPOND` | write | Store an image and prompt the agent with its path |
| `POST` | `/v1/me/fcm-token` | signed | — (self-service) | write | Attach/replace this device's FCM token |
| `POST` | `/v1/workspaces` | signed | `RESPOND` | write | Create a workspace and start a new opencode agent in it |
| `GET` | `/v1/chat/bootstrap` | signed | `READ_STATE` | read | Company, projects, seats, unread counters and chat revision |
| `GET` | `/v1/chat/conversations` | signed | `READ_STATE` | read | Recent conversations, search, pinned and section filters |
| `POST` | `/v1/chat/conversations` | signed | `RESPOND` | write | Create a conversation (Alfred by default) |
| `GET` | `/v1/chat/conversations/{id}` | signed | `READ_STATE` | read | Conversation metadata and participants |
| `POST` | `/v1/chat/conversations/{id}/update` | signed | `RESPOND` | write | Rename, pin, move to a section or archive |
| `GET` | `/v1/chat/conversations/{id}/timeline` | signed | `READ_STATE` | read | Timeline page, newest last, by `seq` |
| `GET` | `/v1/chat/conversations/{id}/messages` | signed | `READ_STATE` | read | Raw message sync by `after_seq`/`before_seq` |
| `POST` | `/v1/chat/conversations/{id}/messages` | signed | `RESPOND` | write | Send a user message; idempotent by `client_id`; `202` |
| `POST` | `/v1/chat/conversations/{id}/read` | signed | `READ_STATE` | none | Move the read cursor to `last_seq` (non-destructive) |
| `GET` | `/v1/chat/sections` | signed | `READ_STATE` | read | User sections and their order |
| `POST` | `/v1/chat/sections` | signed | `RESPOND` | write | Create or rename a section |
| `GET` | `/v1/chat/delegations/{id}` | signed | `READ_STATE` | read | Delegation, its children, its card and its result |
| `POST` | `/v1/chat/delegations/{id}/cancel` | signed | `RESPOND` | write | Cancel one delegation (the history stays) |
| `POST` | `/v1/chat/conversations/{id}/cancel` | signed | `RESPOND` | write | Kill switch: cancel every active delegation of the conversation |
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
gateway already mirrors.

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
gateway calls nothing (updated: this route no longer reaches herdr).

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
- The gateway writes the file under its data dir (`--images`, default
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

> **IMPLEMENTED.** The Android side (`ImageAttachment`, `sendImage`) validates
> the same 5 MiB ceiling and the three types before sending.

### Action responses

Writes return `{"ok": true, "result": {…}}` where `result` is trimmed to what the
app needs (for a prompt: `pane_id`, `agent`, `agent_status`). herdr's own
refusal is a `409`, not a gateway fault (`errors.md`).

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

Response (the gateway's shape):

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

### `POST /v1/me/fcm-token`

Body: `{ "fcm_token": "…" }`, ≤512 chars, non-empty. Response: `{"ok": true}`.

> **IMPLEMENTED (producer + sender, 2026-10-04; real-device E2E pending in M3).**
> The Android build calls it from `GatewayTriageRepository.flushPendingToken()`
> on every connect and on token rotation; the gateway's push sender reads the
> token from the trust store at send time. See [`push.md`](push.md).

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

### `GET /v1/devices` (signed, `ADMIN`)

```json
{"ok": true, "devices": [
  {"device_id":"dev-…","curve":"p256","label":"Pixel 9","owner_id":"12345",
   "created_at":1730000000.0,"last_seen_at":1730000000.0,"revoked":false,
   "permissions":["read_state"]}
]}
```

Public material only: never a `public_key` or an `fcm_token`.

### `POST /v1/devices/{device_id}/revoke` (signed, `ADMIN`)

No body. Response `{"ok": true, "device_id": "dev-…", "revoked": true}`, or
`404 {"detail":"no such device"}`. After a revoke the device's signed requests
answer `401` and its live stream closes within one keepalive.

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

`GET /v1/chat/conversations?q=&section_id=&pinned=&archived=&cursor=&limit=`
lists Recientes: `pinned=false` means "no filter", `archived=false` hides
archived chats, `limit` defaults to 30 (1…100). `q` searches titles and message
bodies (FTS5 when the SQLite build has it, `LIKE` otherwise; the choice is
recorded in `meta.search_mode` and is not part of the wire contract). Response:

```json
{"ok":true,
 "conversations":[{"id":"cnv_01J…","title":"Plan semanal","pinned":false,
   "archived":false,"last_message_at":"2026-10-21T10:00:00.000Z",
   "last_seq":6,"last_read_seq":0,"unread":3,
   "created_at":"…","updated_at":"…"}],
 "next_cursor":"cnv_01J…"}
```

`project_id` and `section_id` appear only when set. `POST
/v1/chat/conversations` takes `{"title":"…","project_id":"prj_…"}` (both
optional; a blank title becomes `Nuevo chat`) and answers
`{"ok":true,"conversation":{…}}`. An unknown project is `422 unknown_project`.

`GET /v1/chat/conversations/{id}` answers
`{"ok":true,"conversation":{…},"participants":[{"agent_id":"…","name":"Alfred","status":"draft"}]}`.

`POST /v1/chat/conversations/{id}/update` takes any subset of
`{"title","pinned","section_id","archived"}`. `section_id: null` clears the
section; an absent field keeps it. Response: `{"ok":true,"conversation":{…}}`.

`GET …/timeline?before_seq=&limit=` pages newest-last by `seq`, default 50
(1…200). Entries carry `seq`, `kind`, `message_id`, `author_kind`,
`author_agent_id` (agents only), `preview` (one line, ≤120 chars), `state` and
`created_at`; `next_before_seq` is the cursor for the next older page. A
message entry has `kind: "message"`; a delegation has `kind: "card"` with
`card_id`, `delegation_id`, `ref_id` (the delegation id), `title`, `summary`
(one line), `status`, `agent_id`, `pane_id`, `attention` and `links` (see
[Delegations and cards](#delegations-and-cards-core7)).

`GET …/messages?after_seq=&before_seq=&limit=` is the raw sync: full `body_md`,
oldest-first when `after_seq` is present, newest-last otherwise (default 100,
1…200).

`POST …/messages` is the turn:

```json
{"client_id":"6d0f…","text":"Crea el proyecto Demo"}
```

`client_id` is required (1…128 characters, no controls), `text` is trimmed,
non-empty, no control characters other than newline/tab, and at most 32 KiB of
UTF-8. The answer is `202` because delivery is asynchronous:

```json
{"ok":true,
 "message":{"id":"msg_01J…","conversation_id":"cnv_01J…","seq":12,
   "author_kind":"user","body_md":"…","state":"complete","created_at":"…"},
 "turn":{"id":"run_01J…","state":"queued","agent_id":"ag_01J…"}}
```

Replaying the same `client_id` returns the first message and turn and does not
deliver the turn again (the same holds if the retry carries different text: the
stored original wins). `202` means the message and the `queued` turn are
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
copies PTY output or invents a reply. The prompt carries only the newest 20
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

**SSE.** `chat.message` and `chat.conversation` travel on the existing
`GET /v1/events` with the existing framing, keepalive and revocation:
`chat.message` carries `conversation_id`, `message_id`, `seq`,
`change: created|updated`, `author_kind` and `preview` — never the full body.
When a run closes, the seat emits `change: updated` for the agent message with
`state: "complete"` (reply committed) or `state: "failed"` (failure card).
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

## Request bodies and signatures

The body used for the signature hash is the **exact received bytes**, not a
re-serialised model. Consequently the server does not require
`Content-Type: application/json`; the bytes are parsed directly as JSON by the
signed-body parser. A correctly signed body with a missing or wrong content type
is still valid.

The signed-body cap is 256 KiB, raised on the image route only to fit a base64
picture (`errors.md`).
