# Contract: device identity, pairing, revocation

Authoritative code: `gateway/src/herdr_notify/api/auth/devices.py`,
`gateway/src/herdr_notify/cli.py` (`add-device`, `enroll`, `devices`),
`android/.../security/DeviceKeyStore.kt`, `android/.../pairing/`.

## Device identity — IMPLEMENTED

Each install generates a P-256 key **inside the Android Keystore**. The private
key never leaves the device, is never exportable, and is never sent anywhere. The
public key is transmitted in the encoding the gateway verifies:

- **P-256:** uncompressed SEC1 point, 65 bytes `0x04 || X(32) || Y(32)`, base64.
- The full curve and encoding rules are in [`auth.md`](auth.md).

The device id is a host-side label (`dev-<16 hex>` by default), not a secret.

## Pairing — IMPLEMENTED

Pairing has two implemented paths, and both land in the same trust store.

**Host-side CLI — IMPLEMENTED.** The operator runs a command on the machine that
runs the gateway. The app's pairing screen cannot submit anything; its whole job
is to present the command with the device's own public key pre-filled.

```
iOS/Android app                    operator's machine
──────────────                     ──────────────────
shows its public key      ──▶      runs:
+ suggested device id                herdr-notify add-device \
                                       --curve p256 \
                                       --public-key <SEC1 base64> \
                                       --label <name>
                                   trust store written atomically
app connects signed       ──▶      running gateway picks up the file
                                   change without a restart
```

```
herdr-notify add-device --curve p256 --public-key <b64> --label <name>
```

The running gateway notices a changed trust store (`reload_if_changed`) and
rebuilds its device list, so pairing a phone does not require restarting the
service.

**HTTP `POST /v1/pair` — IMPLEMENTED (Fase 2(c)).** The one unauthenticated
write, gated by a host-printed code, a per-device recovery code, or tailnet
ownership (see *Enrollment codes* below). It creates a **pending** device that
holds no permission (CORE11, D3): the host grants `READ_STATE` and anything else
afterwards. The wire contract — request body, resolution order, response
and errors — is in [`api.md`](api.md) and [`errors.md`](errors.md);
`gateway/src/herdr_notify/api/app.py` is authoritative.

## Enrollment codes — IMPLEMENTED (Fase 2(c))

Pairing has an HTTP path now: `POST /v1/pair` (see [`api.md`](api.md) and
[`errors.md`](errors.md) for the exact wire contract; `app.py` is
authoritative). It is the **only unauthenticated write**, and it is gated by one
of three proofs, never a static secret:

```
phone                                      host
─────                                      ────
POST /v1/pair {curve, public_key, label, code}
  │
  ├─ code is a live PAIRING code   ──▶  register a new device (code consumed)
  ├─ code is a live RECOVERY code  ──▶  re-key that device, rotate the code
  └─ no code, source IP is the host's own tailnet owner (tailscale whois)
                                    ──▶  register a new device
  ✗ anything else                   ──▶  403 pairing_rejected
```

An **empty pairing code is the exception, not the normal path.** It works only
when the caller's source IP belongs to the host's own tailnet owner; any other
address (a different account, a tagged node, an unavailable daemon, or a request
that arrived through a proxy) is refused with `403 pairing_rejected`. The normal
path is a **code from `herdr-notify enroll`**, which the operator reads off the
host and enters once on the phone.

- **Pairing code**: `DeviceStore.issue_enrollment_code` — 4 random bytes, base32,
  600 s TTL, **single use**. `herdr-notify enroll` prints it. The store validates
  the payload **before** spending the code, so a malformed request cannot deny
  the legitimate phone its pairing. The code is **persisted to the trust store**
  (as a SHA-256 digest, never in the clear), because `enroll` and the running
  `serve` gateway are separate processes: the gateway notices the changed file
  (`reload_if_changed`) and accepts the code. A spent or expired code is dropped
  from the file, so single use survives a restart.
- **Recovery code**: minted at pairing and returned exactly once; stored only as
  a SHA-256 hash. `herdr-notify devices --recovery <id>` mints a new one
  (rotating the old). It re-keys the device without manual approval and cannot
  be used for a revoked device.
- **Auto-approval**: with no code, `tailscale whois --json <source-ip>` must
  report the same `UserProfile.ID` as the host's own `Self.UserID`. A different
  account, a tagged node, or an unavailable daemon never auto-approves. A
  **local `tailscale serve` proxy** makes the request arrive from a loopback
  address, which has no tailnet owner, so it can never auto-approve: run
  `herdr-notify enroll` and use the code.
- A **supplied code** that fails answers the uniform
  `403 {"detail":"pairing_rejected"}` (updated: an empty code now answers a
  slightly clearer `403 {code, message}` naming `herdr-notify enroll`, since the
  caller already knows it sent no code and only the owner check could have
  failed; it still reveals no owner). The endpoint is rate-limited per source IP.

The host-side `add-device` CLI remains the manual path to the same result, and
is the way to grant anything, `READ_STATE` included, to a pending device.

## Trust store — IMPLEMENTED

- Path: `~/.config/herdr-notify/devices.json` (override with `--devices`).
- Shape: `{ "version": 1, "devices": [ {device…} ], "codes": [ {code_hash…} ] }`,
  keys sorted, pretty JSON. `codes` holds the SHA-256 digest and the validity
  window of each **live** pairing code (`{code_hash, created_at, expires_at}`);
  spent and expired codes are not written. The plaintext of a code never appears
  in the file.
- **Fail closed:** a file that cannot be read or parsed leaves the store empty
  and trusts nobody. A corrupt store is never treated as "no devices, so allow
  all".
- **Atomic writes:** written to `*.tmp` then `os.replace`, so a half-written
  store cannot come up believing the wrong devices.
- `touch` (last-seen) writes at most once per device per 60 s; the in-memory
  stamp is always current.

A device record:

```json
{
  "device_id": "dev-…",
  "curve": "p256",
  "public_key": "<base64>",
  "label": "Pixel 9",
  "created_at": 1730000000.0,
  "last_seen_at": 1730000000.0,
  "revoked": false,
  "permissions": [],
  "fcm_token": null,
  "owner_id": "12345",
  "recovery_code_hash": "<sha256 hex>",
  "persisted_at": 1730000000.0
}
```

`persisted_at` is internal bookkeeping and is never returned by `to_public()` /
the API. `recovery_code_hash` is a SHA-256 of the recovery code — the code
itself is never stored; `owner_id` is the `tailscale whois` owner (informational).

## Permissions (authorization) — IMPLEMENTED (pending by default, D3; APPROVE for the UAC decision)

A record carries an explicit grant set — the per-device permissions of
[`auth.md`](auth.md). Pairing grants nothing (CORE11, D3): a new device is
pending, and every route refuses it (`403`) until the host grants it a
permission. The grants are host-side, because widening access is an act
performed by a person at the machine that runs the host, the same reasoning
that keeps enrollment host-side. This applies to the tailnet-owner path too:
an owner match creates a pending device, not a reader.

```
host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]
```

The line above is the **canonical grant hint**: it is exactly the nucleus CLI
(`crates/host/src/cli.rs`, `GRANT_HINT`), and a test keeps it equal to the
command the CLI accepts. Clients that show the operator what to run must quote
it verbatim (Desktop and APK copy it from this file).

- `host devices list` shows each device and its permissions.
- A record without a `permissions` field (pre-M2) loads with `[READ_STATE]`,
  the pairing default before D3 — no re-pairing needed.
- A permission id that is not one of the five is dropped on load: it grants
  nothing rather than guessing.
- An explicitly stored set is exact: an empty set stays empty, and a device
  whose `READ_STATE` was revoked stays refused on the read paths (`403`).
- The per-run hook token of CORE8B is not a device permission and never enters
  the trust store: the host mints it for one run, hands it to the CLI in its
  environment and revokes it when the run closes (`auth.md`).

## LAN TLS pairing link — IMPLEMENTED (CORE11, RFC-transports L1)

The LAN listener is off by default. `host --tls-bind <ip:port>` serves the
device routes over TLS 1.3 on a concrete address; a wildcard is refused, as
`--bind` refuses it. The hook routes and the internal MCP surface are never
mounted on this listener, so a LAN device cannot reach them.

`host pair-code --lan <ip:port>` (repeatable) prints the code and a pairing link:

```
herdr-notify://pair?v=1&c=<code>&h=<pin>[&b=<backup pin>]&a=<ip:port>...&e=<unix seconds>
```

| Field | Meaning |
|-------|---------|
| `v` | Link version, `1`. Any other version is refused. |
| `c` | The single-use pairing code. It is the secret: only its hash is stored. |
| `h` | Pin of the active certificate: SHA-256 of its SubjectPublicKeyInfo, base64url without padding (43 characters). |
| `b` | Optional backup pin, present only while a certificate is prepared, or after an activation (then the previous pin). |
| `a` | Addresses the device may reach, concrete `ip:port` only. Repeatable. |
| `e` | Expiry, unix seconds (`pair-code --ttl`). |

Parsing is strict: an unknown or duplicated field refuses the link. The
device pins the key, not a name: it accepts a server that presents the pinned
key, and does not check a hostname or a CA. The certificate is self-signed
ECDSA P-256, valid for 10 years under `<data-dir>/tls/`; the pinned verifier
also refuses a certificate outside its validity window. The pins are public;
the code is not.

The host prints the link as text. A QR rendering belongs to the Desktop and the
APK, so the host carries no QR dependency.

**Rotation.** `host tls prepare` creates the next certificate and prints its
pin; links printed afterwards carry it as `b`. `host tls activate` makes it the
served certificate and keeps the previous pin as the backup of later links. The
daemon must restart to serve it. A device accepts a pin only if it was given
it, so a device reaches the new certificate only if it paired (or re-paired)
while the new pin was the backup. A device paired earlier holds only the old
pin and must pair again.

**Phone side (APK, CORE11B).** The phone reads the link in the shared Rust core
(`apk/mobile/rust`, `api/pinned.rs`) and refuses an expired `e`, an unknown `v`
and a malformed `h` with a typed error. With `h` present, every connection to the
host is TLS 1.3 and the server key must match `h` or `b`; TLS 1.2 is refused. A
pin mismatch is final: the phone does not try the next `a=` address (only a TCP
connect failure moves on). The pins are stored with the pairing. A link without
`h` keeps the earlier unpinned path, and the pairing screen states when that
traffic is unencrypted outside loopback.

## Revocation — IMPLEMENTED

```
herdr-notify devices                 # list, with REVOKED markers
herdr-notify devices --revoke <id>   # revoke one device
```

Since Fase 2(c), an `ADMIN`-granted device can also do this over HTTP:
`POST /v1/devices/{device_id}/revoke` (and `GET /v1/devices` to list).

- Revocation **marks**, it does not delete. A revoked device stays visible and
  refused, which is easier to audit than one that quietly becomes unknown and
  could be re-enrolled under the same id.
- Revoking clears the device's `fcm_token`.
- Effect is immediate: `RequireDevice` checks `revoked` before any cryptography,
  so the next request gets `401`.
- Revoking one device does not affect any other device's key.

## Migrar desde el gateway Python

Un dispositivo emparejado con el gateway Python (`gateway/`, obsoleto y solo
de lectura) pasa a este host en tres pasos:

1. Copiar el `devices.json` del gateway al host. El importador no lo modifica.
2. `host devices import-gateway <devices.json> [--force] [--data-dir DIR]`
3. El teléfono se re-empareja con su **código de recuperación** enviándolo como
   `code` en `POST /v1/pair`. Conserva su `device_id` y sus permisos, y recibe
   un código de recuperación nuevo.

Reglas del importador:

- Se importan el `device_id`, la clave pública y su curva, la etiqueta, los
  permisos, la revocación y el hash del código de recuperación.
- No se importan el token FCM ni los códigos de emparejamiento pendientes.
- Un permiso desconocido se descarta con aviso; el dispositivo se conserva.
- Una entrada corrupta o con clave inválida se informa como rechazada; el resto
  se importa igual.
- Mismo `device_id` con otra clave: se conserva la clave del host salvo con
  `--force`. La misma clave bajo otro id no se duplica.
- Reimportar sin cambios no escribe nada. La escritura es atómica: si falla, el
  fichero del host queda como estaba.
- Un dispositivo sin `recovery_code_hash` no puede re-emparejarse; debe volver a
  emparejarse con un código de emparejamiento.

## Implementation status

| Capability | Status |
|------------|--------|
| Device identity (P-256 / Ed25519 public key in the keystore) | IMPLEMENTED |
| Host-side `add-device` CLI pairing | IMPLEMENTED |
| `POST /v1/pair` (code / recovery / tailnet owner) | IMPLEMENTED (Fase 2(c)) |
| Pairing codes (4 bytes, 600 s, single use) | IMPLEMENTED |
| Recovery codes (re-key, rotate, SHA-256 hash only) | IMPLEMENTED |
| Tailnet auto-approval (`tailscale whois`, same owner) | IMPLEMENTED |
| Trust store (atomic writes, fail closed, live reload) | IMPLEMENTED |
| Pending device on pairing (no permission until the host grants one, D3) | IMPLEMENTED (CORE11) |
| LAN TLS listener, pinned pairing link, pin rotation (`tls prepare` / `activate`) | IMPLEMENTED (CORE11) — see *LAN TLS pairing link* |
| Device management (`GET /v1/devices`, `POST /v1/devices/{id}/revoke`) behind `ADMIN` | IMPLEMENTED (Fase 2(c)) |
| Tailnet peer list (`GET /v1/peers`, `503` when unavailable) | IMPLEMENTED |
| HTTP grant endpoint | NOT IMPLEMENTED — widening access is a host-side CLI act by design |
| `APPROVE` permission endpoints | IMPLEMENTED (CORE8) — `POST /v1/chat/approvals/{id}/decision`, the UAC decision |
