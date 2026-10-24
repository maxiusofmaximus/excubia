# Contract: error model

Authoritative code: `gateway/src/herdr_notify/api/{app,actions,ratelimit}.py`.

## Status codes

| Status | `detail` | When |
|--------|----------|------|
| `400` | `invalid_body`, or `{code, message}` from an action | Body is not JSON / missing field; or an action failed validation |
| `401` | `"unauthorized"` | Any authentication rejection (uniform — see below) |
| `403` | `"forbidden"` | Any authorization rejection (uniform — see below) |
| `409` | herdr's own message | herdr declined the action (legitimate answer, not a fault) |
| `413` | `"body too large"` | Body exceeds the cap (8 KB enroll/pair, 256 KB signed, image route's own larger cap) |
| `422` | `{ "code": …, … }` | A signed body failed model validation (see below: `invalid_body`, and the dedicated `/v1/workspaces` codes) |
| `429` | `"slow down"` / `"too many open streams"` | Rate limit, or per-device stream cap |

`429` responses carry `Retry-After` (integer seconds).

## Pairing (Fase 2(c))

`POST /v1/pair` is the one unauthenticated write. Its failures are:

| Status | `detail` | When |
|--------|----------|------|
| `400` | `"invalid_body"` | not JSON, missing `curve`/`public_key`, unsupported curve |
| `403` | `"pairing_rejected"` (string) | a supplied code/recovery code is unknown, expired, used, or the device is revoked |
| `403` | `{ "code": "pairing_rejected", "message": … }` | no code was supplied and the source IP is not the host's tailnet owner; Tailscale unavailable |
| `413` | `"body too large"` | body > 8 KiB |
| `429` | `"slow down"` | per-source-IP limit (`Retry-After`) |

For a **supplied code**, `pairing_rejected` is **uniform**: the caller is never
told which code or which check failed, the same oracle reasoning as the uniform
`401`/`403`. When **no code** was supplied there is only the tailnet-owner check
to fail, and the caller already knows it sent no code, so the refusal is a
`{code, message}` object whose message names the working path
(`herdr-notify enroll`) and, for a loopback source, the local-proxy cause. It
never reports whose account owns an address.

## Order of checks

A request is evaluated in this order, cheapest and most security-relevant first:

1. Signature and freshness → `401` (`auth.md`).
2. Authorization against the endpoint matrix → `403` (`auth.md`).
3. Read rate limit → `429`.
4. For writes, write rate limit → `429`.
5. Body size → `413`; body parse → `400`/`422`.
6. Action validation → `400` with an action code.
7. herdr → `409` if it refuses.

An authorization failure happens before the rate limit and before `last_seen`
is updated: an unauthorized request consumes no budget and leaves no trace.

## The uniform 401

Every authentication rejection — no headers, unknown device, revoked, bad
timestamp, bad nonce, bad signature, malformed, unsupported curve — returns the
same `401` with `{"detail": "unauthorized"}`. The specific reason is logged
server-side only. Distinguishing "unknown device" from "bad signature" would turn
the endpoint into an oracle for which device ids exist.

## The uniform 403

Every authorization rejection — a granted permission missing (`not_permitted`),
an endpoint with no matrix rule (`unknown_endpoint`) — returns the same `403`
with `{"detail": "forbidden"}`. The reason, the device id and the required
permission are logged server-side only: the log is the audit trail.

`401` and `403` are different answers and the client must not conflate them:

- `401` — this device is no longer trusted. Terminal: stop retrying, re-pair.
- `403` — this device is trusted but not allowed to do this. Terminal **for
  that action**: stop retrying it, do **not** re-pair (the pairing is intact),
  do **not** treat it as a transport error. The fix is a host-side grant, never
  a client-side retry.

An authorization error is also **not** a triage status: a triage status is an
agent's own state (`state.md`), not an HTTP outcome. herdr's vocabulary has no
`ERROR` (D2 resolved it: herdr cannot produce one, the contract drops it), so
there is no triage `ERROR` state for an authorization error to be confused with.
The concepts are distinct and never share a representation.

## Action error codes

Returned in the `400` body as `{ "code": "...", "message": "..." }`.

| `code` | Meaning |
|--------|---------|
| `bad_pane` | The target is not a pane id |
| `pane_gone` | The pane is no longer known: its agent process is gone and the pane left the state list |
| `no_keys` | An empty key list |
| `too_many_keys` | More than 8 keys |
| `empty_key` | A blank key in the list |
| `confirmation_required` | A guarded key (`ctrl+c`) without `confirm: true` |
| `key_not_allowed` | A key outside the allowlist |
| `empty_prompt` | A blank prompt |
| `control_characters` | A prompt with terminal control characters |
| `unsupported_type` | An image whose declared type is not png/jpeg/webp, or whose bytes are none of them |
| `invalid_image` | Image `data` is not valid base64 |
| `empty_image` | Image `data` decodes to zero bytes |
| `image_too_large` | Decoded image over 5 MiB |
| `type_mismatch` | Declared image type does not match the bytes |

There is no `prompt_too_long` code any more: prompts have no length cap
(2026-10-13).

`pane_gone` (decision 2026-10-19) is the one code that describes a *target*
that used to exist: the agent's process is gone and the pane has left the
state list, so the row the client holds is stale. It is a `400` with the
usual `{code, message}` body; the client should drop the row and offer to
start a new conversation rather than retry. A pane id that was never valid is
still `bad_pane`. A pane restored from a previous host run (additive
`restored` field in `state.md`) is *not* `pane_gone`: the row exists, `read`
serves its last screen, and driving it answers `409` (`no running agent`).

Stopping a pane (`POST /v1/agents/{pane_id}/stop`) is the one action whose
success *ends* the pane: the process is closed, the pane is removed and every
later action answers `pane_gone` — including a second stop, which is therefore
`pane_gone` rather than an idempotent `200` (the client's retry lands on a
pane that no longer exists). The success response is `{"ok": true,
"pane_id": …}`, and the stream carries `pane.closed`.

These codes are stable identifiers the app may branch on. They are not
localised; the app renders its own text.

## Model errors with dedicated codes (2026-10-20)

`POST /v1/workspaces` answers its four model failures with a stable `code`, an
explicit `field` and a short `message`, all inside `detail`:

```json
{"detail": {"code": "unknown_agent", "field": "agent",
            "message": "unknown agent \"foo\"; the host knows: claude, codex, opencode"}}
```

| `code` | `field` | When |
|--------|---------|------|
| `unknown_agent` | `agent` | the name is not in the host's catalogue |
| `agent_unavailable` | `agent` | the name is known but its command does not resolve on this host |
| `invalid_cwd` | `cwd` | the path does not exist, or is not a directory |
| `invalid_label` | `label` | more than 64 characters, or control characters (an empty or blank label is simply omitted) |

Everything else keeps the previous shape: a body that is not JSON is
`400 {"detail": "invalid_body"}`, and JSON of the wrong shape is
`422 {"detail": {"code": "invalid_body", "errors": [...]}}` with the
pydantic-style `loc`/`msg` list.

**What Desktop and APK must change:** they currently branch on `loc[0]` for
every 422. Read `detail.code` first: when it is one of the four codes above,
use `detail.field` to highlight the input and show `detail.message`; keep the
`loc[0]` path only as the fallback for `invalid_body`. The HTTP status does not
change (`422`), the route's success shape does not change, and no other route
is affected.

## Chat errors (`/v1/chat`, CORE6A/CORE6B/CORE7)
| Status | `detail` / `code` | When |
|--------|-------------------|------|
| `400` | `{ "code": "empty_message", "message": … }` | `text` is blank after trimming |
| `400` | `{ "code": "control_characters", "message": … }` | `text` has a control character other than newline/tab |
| `400` | `{ "code": "invalid_cursor", "message": … }` | `before_seq`/`after_seq` not a non-negative integer, or a malformed `Last-Event-ID` |
| `404` | `"no_such_conversation"` | the id is unknown to this host or belongs to another company |
| `404` | `"no_such_delegation"` | the delegation id is unknown to this host |
| `409` | `"conversation_archived"` | writing a message to an archived conversation |
| `409` | `"seat_unavailable"` | no supported CLI adapter is installed, or the seat is stopped; the turn is not delivered and its message is not kept in the conversation |
| `409` | `"agent_busy"` + `Retry-After: 5` | the seat's pending queue is full; the turn is not delivered and its message is not kept: send the same text again with the same `client_id` after `Retry-After` seconds |
| `409` | `"delegation_cancelled"` | the delegation is not active (already done, failed or cancelled) |
| `409` | `"scope_denied"` | the seat may not do this: a specialist cannot delegate, Alfred does not create a second lead |
| `409` | `"depth_exceeded"` | the delegation would be deeper than two levels |
| `409` | `"cycle_detected"` | the delegation repeats a `(from, to, task)` pair of the last ten minutes, or delegates to itself |
| `409` | `"turn_budget"` | the seat already made twelve delegations in this turn |
| `409` | `"hour_budget"` | the seat already made sixty delegations in the last hour |
| `409` | `"seat_limit"` | the project already has eight seats |
| `409` | `"lead_exists"` | the project already has a lead |
| `409` | `"project_exists"` | there is already a project at that path |
| `404` | `"no_such_approval"` | the approval id is unknown to this host |
| `409` | `"approval_expired"` | the TTL passed without a decision |
| `409` | `"approval_already_decided"` | another device decided over an approval that is already resolved, or the approval was already consumed |
| `409` | `"approval_mismatch"` | the digest of the consumption is not the one the approval was opened with |
| `409` | `"approval_digest_reused"` | the action was already approved: the same digest never opens a second approval |
| `409` | `"approval_not_approved"` | the approval is not `approved`: it cannot be consumed |
| `409` | `"approval_limit"` | the conversation already has four approvals waiting for a decision |
| `403` | `"forbidden"` | `POST /v1/chat/approvals/settings` without `APPROVE` (`ADMIN` does not replace it), or from a hook token: the uniform `403` |
| `422` | `{ "code": "invalid_mode", "field": "mode", "message": … }` | the approval mode is neither `delegated` nor `always_user` (CORE9-D) |
| `422` | `{ "code": "unknown_project", "field": "project_id", "message": … }` | an approval-mode change names a project that does not exist (CORE9-D) |
| `422` | `{ "code": "unknown_project", "field": "project_id", "message": … }` | creating a chat for a project that does not exist |
| `422` | `{ "code": "unknown_section", "field": "section_id" }` | moving a chat to a section that does not exist (`field: "id"` when creating/renaming a section by id) |
| `422` | `{ "code": "invalid_body", "field": …, "message": … }` | a generic field is invalid: `text` over 32 KiB, `title`, `name`, `client_id`, `last_seq`, `limit`, or a non-boolean flag |

Everything else keeps the existing order: signature `401`, authorization
`403`, budget `429`, then body validation. The chat `422`s use the CORE5
`{code, field, message}` shape; `invalid_body` is still the generic code for a
field with no dedicated one.

The delegation refusals (`scope_denied`, `depth_exceeded`, `cycle_detected`,
`turn_budget`, `hour_budget`, `seat_limit`, `lead_exists`, `project_exists`,
`delegation_cancelled`) are the same codes in the MCP surface, where they are
`409` with `detail.code` and no `field`.

## Approval errors (CORE8)

The approvals of `/v1/chat/approvals` answer with the same shapes as the rest
of the chat surface: `404` with `"no_such_approval"` for an unknown id and
`409` with the code in `detail` for a state conflict.

| `code` | When |
|--------|------|
| `approval_expired` | the TTL (120 s by default, 300 s at most) passed without a decision |
| `approval_already_decided` | another `client_id` decides over an approval that is already resolved, or the approval was already consumed |
| `approval_mismatch` | the digest of the consumption is not the one the approval was opened with |
| `approval_digest_reused` | the same action is asked again: a digest opens one approval, never two, not even after a denial, an expiry or a consumption. The one exception is coalescing: the same seat asking again while its approval is still `pending` gets that approval back (no error) |
| `approval_not_approved` | the approval is `pending`, `denied` or `expired`: it cannot be consumed |
| `approval_limit` | the conversation already has four approvals waiting for a decision |

The hook routes add three `403` with the code in `detail` (they are not the
uniform device `forbidden`: the hook logs them and fails closed):

| `code` | When |
|--------|------|
| `hook_token_revoked` | the token belongs to a run (or panel) that already closed |
| `hook_token_not_scoped` | the host-wide token was presented on an approval route: it names no run |
| `hook_scope_mismatch` | the token is valid but the approval belongs to another run or seat; nothing changes |

The MCP tool `approval.request` refuses with the same codes: `approval_denied`
(the operator refused), `approval_expired` (nobody answered in time) and
`approval_not_approved`. The blocking hook answers with an exit code instead:

| exit | meaning |
|------|---------|
| `0` | the action may run (the approval was consumed just before) |
| `2` | denied: the operator refused, the approval expired, the host never answered (timeout, refused connection, malformed response), or the binary was called wrong (invalid argument) |

`2` is the only "no" a CLI can act on. Every host failure and every bad
invocation is a `2`, because a CLI that read another code as "the hook broke"
could let the action through.

Idempotency is not an error: the same `client_id` returns the stored message and
turn (`202`) and never redelivers the turn. Both seat refusals mark the stored
run `failed` without adding a failure card; the client retries with a new
`client_id`. A turn accepted with `202` can still end `failed` later (CLI exit,
timeout or a malformed `chat.reply`); that is a run outcome, not an HTTP error,
and it does add the empty-message failure card.

The internal MCP endpoint is not part of this public contract. It answers `401`
for a missing/unknown per-run token and `422` with the same
`{code, field, message}` detail for invalid tool arguments; the MCP shim turns
those into an `isError` tool result. The CORE7 tools add these codes:
`invalid_arguments` (with the offending `field`), `unknown_tool`, `invalid_path`
(the project path leaves `workspaces_root`), `pane_limit` (six live panels
already), `seat_unavailable` (no panel for the seat) and `not_your_delegation`
(a seat completing a delegation that is not its own).

## Approval delegation (CORE9-B)

The MCP tool `approval.decide` lets a lead decide the approvals of its project's
seats (level `lead`) and Alfred decide the approvals at its level (`cto`). A
seat that asks again for an action it still waits on gets the same approval
back (the digest is the identity), not an error. The refusals, all with
`detail.code` and no `field` unless noted:

| `code` | Status | When |
|--------|--------|------|
| `approval_scope_denied` | `403`, on HTTP and MCP (`field: "approval_id"`) | the caller is not a lead or Alfred, or the requester is outside the lead's project |
| `approval_level_mismatch` | `409` | the approval sits at a level this seat does not decide |
| `approval_not_delegable` | `409` | the approval is `never_delegate` (level `user`) |
| `approval_self_approval` | `409` | the seat asked for it: same `seat_id`, or the same non-empty `run_id` |
| `approval_budget` | `409` | six delegated decisions by this agent in the last hour |
| `approval_expired`, `approval_already_decided` | `409` | as in the approval errors above |
| `no_such_approval` | `422` on MCP (`field: "approval_id"`) | the approval id is unknown |

Each decision writes one `approval_audit` row whose `decider_kind` is `agent`,
`decider_id` the deciding seat and `level` the level it decided. An expiry
writes `decider_kind` `system`. The MCP answer is `{"approval": …}`.

The MCP tool `approval.pending` takes no arguments and answers
`{"approvals": […]}`: the approvals waiting at the caller's level. A lead gets
its project's, Alfred the company's, and the caller's own requests are never
listed. A seat with no such level is refused with `403 approval_scope_denied`.

## Deliberately not distinguished

- A malformed body is a client error (`400`/`422`), never a `500`. A phone that
  cannot tell a refusal from a server fault would keep retrying a request that
  can never succeed.
- herdr's refusal (`409`) is a normal outcome. An agent that is no longer blocked,
  or a prompt herdr declined, is information, not a gateway failure.
