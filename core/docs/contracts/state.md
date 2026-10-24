# Contract: state model and triage

`herdr` events are reduced on the host into a read model. The app renders that
model; it does not re-derive it. Authoritative code:
`gateway/src/herdr_notify/state/{models,store}.py`,
`gateway/src/herdr_notify/notify/attention.py`,
`gateway/src/herdr_notify/api/app.py` (`state_payload`),
`android/.../net/Wire.kt`, `android/.../domain/SessionState.kt`.

## One shape, two transports

`GET /v1/state` and the **opening frame** of `GET /v1/events` return the exact
same object, built by one function (`state_payload`). This was not always true:
the stream used to return the raw reduced view, which has no `attention` and a
different `counts`, so a phone that joined the stream ranked from a payload the
gateway never ranked. That divergence is fixed and pinned by a test.

## The state object

```json
{
  "revision": 42,
  "herdr_version": "0.9.3",
  "focused": { "workspace_id": "w1", "tab_id": "t1", "pane_id": "w1:p1" },
  "counts": { "blocked": 1, "working": 2, "idle": 3 },
  "agents": [ /* TriageRow, ranked */ ],
  "chat": { "revision": 58, "unread": 2, "pending_approvals": 0, "pending_user_approvals": 0, "active_delegations": 0 }
}
```

- `chat` is additive (CORE6B). Its `revision` is the chat journal position, not
  the state revision; `unread` counts unread agent messages and
  `active_delegations` counts the delegations that are `queued`, `running` or
  `waiting` (CORE7). The opening SSE `state` frame carries the same block,
  built by the same helper as `GET /v1/state`.
- `pending_approvals` counts the approvals that wait for a decision (CORE8).
  It is live: it moves when an approval is opened, decided or expires, without
  any new polling.
- `pending_user_approvals` (CORE9-D) counts the approvals that wait at the
  operator (`level` `user`). It moves with the approvals and at once with an
  approval-mode change: a switch to `always_user` raises it by the approvals that
  it sends up.
- `revision` is monotonic over the reduced state. An older revision arriving
  after a newer one is discarded by the client.
- `counts` is a **tally per status**, containing only statuses that are present.
- `agents` is ordered by the ranking below, not by pane id.

### Agent entry (TriageRow)

```json
{
  "pane_id": "w1:p1",
  "workspace_id": "w1",
  "project": "herdr-notify",
  "agent": "opencode",
  "status": "blocked",
  "attention": 3,
  "title": "Add the pairing screen",
  "cwd": "<ruta-del-repo>",
  "focused": false,
  "can_resume": true
}
```

| Field | Notes |
|-------|-------|
| `pane_id` | The key every action is addressed by. An entry without one is dropped by the client. |
| `project` | Workspace label, falling back to a project name, then `?`. |
| `agent` | Display name; `display_agent` overrides the agent kind. |
| `status` | One of the statuses below. |
| `attention` | Ranking 0–3, computed on the host. |
| `title` | Best available human label; may be `null`. |
| `focused` | herdr's focus flag; a tiebreaker, never a promotion. |
| `can_resume` | Whether the agent reported a native resume reference. |
| `restored` | Additive and optional (2026-10-19): present as `true` only on panes recovered from a previous host run. Their process is gone (`status` is `unknown`), `read` serves the last screen that was kept, and driving them answers `409`. Older clients ignore the field. |

## Statuses

`idle` · `working` · `blocked` · `done` · `unknown`

This is herdr's own vocabulary, forwarded unchanged. An unrecognised value
degrades to `unknown`; it is never stored as-is and never raises.

## Attention and ranking

| Status | Attention | Value | Pushed? |
|--------|-----------|-------|---------|
| `blocked` | `URGENT` | 3 | yes, high priority |
| `done` | `REVIEW` | 2 | yes, configurable |
| `working` | `INFO` | 1 | no |
| `unknown` | `INFO` | 1 | no |
| `idle` | `NONE` | 0 | no |

Sort key, ascending: `(-attention, focused ? 0 : 1, project.lower(), pane_id)`.
Urgency first; the focused pane breaks ties but never outranks a `blocked` agent
elsewhere; then stable name order.

The `chat` block of the same object carries the live counters of the chat
surface. `pending_approvals` (CORE8) is the number of approvals that wait for
a decision: it is the UAC inbox seen from the state, and it moves with the
`chat.approval` frames.

## The SSE stream

`GET /v1/events`, authenticated once at connect. The signature covers the
connect request only; a long-lived stream is not re-verified per frame. A
reconnect is a **new signed request with a fresh nonce**; it may additionally
resume the chat journal with `Last-Event-ID`, but state is always resent in
full by the opening frame.

Frame format:

```
event: state
data: {"event":"state","revision":42,"state":{ …the state object… }}

```

- **First frame** is always `event: state` carrying the full state object, so a
  client that connects while changes are queued still converges without a second
  round trip. This is the app's revision baseline.
- **Change frames** carry `event: <name>` and a JSON body containing at least
  `revision`. The app does not patch locally; it re-reads `/v1/state`, so the
  phone and the terminal cannot drift.
- **Keepalive** is the comment frame `: keepalive` every 20 seconds, so
  intermediaries do not time the connection out.
- Response headers: `Cache-Control: no-store`, `X-Accel-Buffering: no`.
- Per-device stream cap; exceeding it is `429` (`errors.md`).

### Chat frames and resumption (CORE6A/CORE6B/CORE7)

The same stream carries `event: chat.message` and `event: chat.conversation`.
Their JSON body has a global, monotonic `revision` and a `preview`, never the
full message body; the client refreshes the affected timeline or conversation.

```
id: 58
event: chat.message
data: {"event":"chat.message","revision":58,"conversation_id":"cnv_…","message_id":"msg_…","seq":12,"change":"created","author_kind":"user","preview":"Crea el proyecto…"}
```

- The SSE `id` is the chat revision. It is the only resumable position: state
  frames carry no id and are not replayed.
- On reconnect, `Last-Event-ID` replays journaled chat frames after that id,
  oldest first, before any live frame. The subscription is opened before the
  journal is read and duplicate live frames are dropped, so an event is never
  delivered twice on one connection.
- Replay is bounded (10 000 frames). A client beyond the bound reloads the
  timeline, which is the authoritative view; this is the same resync rule as a
  dropped state queue.
- A malformed or negative `Last-Event-ID` is `400 invalid_cursor`.
- When a run closes, the `chat.message` frame for its agent message carries
  `change: "updated"` and `state: "complete"` or `state: "failed"`; a turn
  accepted with `202` is `queued` until the seat starts it.
- `/v1/state` and the opening SSE frame include the additive `chat` block
  (`revision`, `unread`, `pending_approvals`, `pending_user_approvals`,
  `active_delegations`); bootstrap carries the same shape.
- CORE7 adds `event: chat.delegation` (a delegation was created) and
  `event: chat.activity` (its state changed: `queued`, `running`, `waiting`,
  `done`, `failed` or `cancelled`). Both carry `conversation_id`,
  `delegation_id`, `card_id`, `status`, `pane_id` and a one-line `summary`;
  the client refreshes the timeline, where the delegation is a `kind: "card"`
  entry.
- The state of a delegation follows the pane, not a new poll: a pane that goes
  `blocked` puts the delegation in `waiting` (the card asks for attention) and
  a pane that works puts it in `running`. A panel that closes fails its
  delegation; the seat of a panel that disappears stops pointing at it.
- A host restart marks the delegations that were `running` or `waiting` as
  `interrupted` and forgets the pane of every project seat. No panel is
  relaunched by itself.
- CORE8 adds `event: chat.approval` (an approval was opened, decided,
  consumed or expired). It carries `conversation_id`, `approval_id`,
  `agent_id`, `pane_id`, `status`, `kind`, `summary` and `expires_at`; the
  client refreshes the UAC inbox. CORE9-C adds `level`, `escalated_from`,
  `decider_kind`, `decider_id`, `reason` and `change`, which is `created`
  (pending), `escalated` (a level ran out or was skipped), `decided`,
  `consumed` or `expired`. An approval decided below the operator also leaves
  a timeline entry of `kind: approval` with its `status`, its `summary` and
  its decider; the operator's own decisions have a card instead.
- CORE9-D adds `event: chat.approval_settings` (the approval mode changed). Its
  body is `{event, scope, mode, previous, escalated}`: `scope` is `global` or a
  project id, `previous` is the mode it leaves, and `escalated` counts the
  approvals that the switch sent to the operator. It is journaled like the other
  chat frames.
- A host restart expires the approvals that were `pending`: nobody will ever
  answer them, so they die with the process and the audit keeps the trace as
  the host's own decision (`decider_kind` `system`, `decider_id` `host`). It
  also revokes every hook token of the previous process (`auth.md`), so a hook
  that outlived its run cannot answer for it.

The server drops from the queue of a client that stops reading rather than
growing memory; that client re-syncs from `/v1/state` on reconnect and from the
timeline for chat.

## Client decoding

The app decodes leniently (`ignoreUnknownKeys`, `isCoerceInputValues`): a newer
gateway adding a field must not stop an older phone. A malformed body falls back
to the last good state and shows it as stale, rather than emptying the screen.

## `ERROR` — dropped by D2 (herdr never reports it)

The specification (§10) lists `ERROR` among the minimum states. herdr cannot
produce one: in herdr 0.9.1 (`src/api/schema/common.rs`) `AgentStatus` is
`idle | working | blocked | done | unknown` — no `Error` arm — and
`PaneAgentState`, the raw detection state, has none either; `parse_agent_status`
(`src/cli.rs`) accepts only those five strings. The gateway forwards herdr's
status untouched and degrades an unrecognised value to `unknown`, so an `ERROR`
state is unreachable by construction and no `ERROR` was added to the model
(decision D2 in [`../roadmap.md`](../roadmap.md)). If a future herdr adds an
error-like state, this contract extends — additive changes only.
