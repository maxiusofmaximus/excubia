# Contract: device identity, pairing, revocation
_(implementado en el núcleo Rust; ver README)_

Authoritative code: `crates/auth/src/store/`, `crates/host/src/cli/`.

## Device identity — IMPLEMENTED

Each install generates a P-256 key in Rust (`apk/mobile/rust`, `signing.rs`). The
app keeps the private key in `flutter_secure_storage` (encrypted at rest with a key
held by the Android Keystore); the key itself is **not** generated inside the
Keystore, so it is not hardware-bound or non-exportable. It is never sent anywhere.
The public key is transmitted in the encoding the host verifies:

- **P-256:** uncompressed SEC1 point, 65 bytes `0x04 || X(32) || Y(32)`, base64.
- The full curve and encoding rules are in [`auth.md`](auth.md).

The device id is a host-side label (`dev-<16 hex>` by default), not a secret.

## Pairing — IMPLEMENTED

Pairing has two implemented paths, and both land in the same trust store.

**Host-side CLI — IMPLEMENTED.** The operator runs a command on the machine that
runs the host. The app's pairing screen cannot submit anything; its whole job
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
app connects signed       ──▶      running host picks up the file
                                   change without a restart
```

```
herdr-notify add-device --curve p256 --public-key <b64> --label <name>
```

The running host notices a changed trust store (`reload_if_changed`) and
rebuilds its device list, so pairing a phone does not require restarting the
service.

**HTTP `POST /v1/pair` — IMPLEMENTED (Fase 2(c)).** The one unauthenticated
write, gated by a host-printed code, a per-device recovery code, or tailnet
ownership (see *Enrollment codes* below). It creates a **pending** device that
holds no permission (CORE11, D3): the host grants `READ_STATE` and anything else
afterwards. The wire contract — request body, resolution order, response
and errors — is in [`api.md`](api.md) and [`errors.md`](errors.md);
`crates/api/src/routes/` is authoritative.

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
  180 s TTL (the host default; `host pair-code --ttl` overrides it), **single use**. `herdr-notify enroll` prints it. The store validates
  the payload **before** spending the code, so a malformed request cannot deny
  the legitimate phone its pairing. The code is **persisted to the trust store**
  (as a SHA-256 digest, never in the clear), because `enroll` and the running
  `serve` daemon (`host`) are separate processes: the host notices the changed file
  (`reload_if_changed`) and accepts the code. A spent or expired code is dropped
  from the file, so single use survives a restart.
- **Recovery code**: minted at pairing and returned exactly once; stored only as
  a SHA-256 hash. `herdr-notify devices --recovery <id>` mints a new one
  (rotating the old). It re-keys the device without manual approval and cannot
  be used for a revoked device.
  The host CLI mints one the same way: `host devices recovery <device_id>
  [--data-dir DIR]` prints the new code once and replaces the old one. An
  unknown or revoked device is refused with an error and no code is minted.
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

**Pairing audit.** Every decision of `POST /v1/pair` that reaches the handler,
and every grant of `POST /v1/devices/{device_id}/grant`, appends one line to
`<data-dir>/pairing-audit.jsonl`:
`{ts, outcome: paired|recovered|rejected|granted, device_id, label, via: code|recovery|tailnet|admin, source_ip}`.
A `granted` line also carries `granted_by` (the admin device) and `permissions`.
A refusal records `device_id: null`, and its `via` is `code` when a code was
sent, `tailnet` otherwise. No line holds a code, a public key or a verification
string. A failed write is reported on stderr and does not fail the pairing.

**Rotation.** An append that would pass 1 MiB first moves the file to
`pairing-audit.jsonl.1`, replacing the previous one, and a new file starts. Both
happen under the write lock, so no line is lost. A failed rotation is reported on
stderr and the line is still written.

**Pairing notice.** A `paired` or `recovered` decision, and a grant, also posts a
message from Alfred in the conversation «Avisos del sistema». The conversation is
created by the first notice and found by its title among the conversations not
archived; the host keeps its id until it restarts, so an archived one keeps the
notices until then. The texts are:

- ``Nuevo dispositivo pendiente: `<label>` (<device_id>), sin permisos. Cadena de verificación: **<sas>** (debe ser la que muestra el móvil). Si no fuiste tú: `host devices revoke <device_id>`.``
- ``Se usó el código de recuperación de <device_id> (`<label>`). Si no fuiste tú: `host devices revoke <device_id>`.``
- ``` `<granter>` concedió <permisos> a `<label>` (<device_id>). ``` — `<granter>` is the admin device's id, `<permisos>` the permissions, comma-separated.

The sentence with `<sas>` is left out when the host has no pin to compute it
(see *Verification string* below). The label is a code span cut to 40 characters,
with line breaks turned into spaces and backticks into quotes. A refusal posts
nothing. A failed notice is reported on stderr and never fails or undoes the
pairing or the grant.

## Verification string (SAS) — IMPLEMENTED (F1-núcleo)

A pending device gets a six-digit string that the phone shows as well. The Master
compares the two before granting anything, so he checks that the phone in his hand
is the one being paired (RFC `pairing-security` §5, F1). Both sides compute it from
public material only:

```
sas = six digits of ( u32_big_endian( SHA-256( "excubia-sas-v1" ‖ 0x00 ‖ host_pin ‖ device_public_key )[0..4] ) mod 1 000 000 )
```

- `host_pin`: the 32 bytes behind the active certificate's pin (the `h` of the link, base64url-decoded).
- `device_public_key`: the bytes the device sent in `POST /v1/pair`, after base64 decoding: the SEC1 point, 65 bytes for P-256.
- Six digits with leading zeros kept, so the output is always 6 characters.

Implementation: `hn_core_pin::sas` (crate `pin`, which has no host dependencies, so the
APK depends on it by path). Where it is shown:

- the notice of a `paired` decision (above);
- `host devices list`, on a line under each pending device: `cadena de verificación: <sas>`;
- `GET /v1/devices`, field `sas`, on each device that holds no permission and is not revoked.

It is not written to the audit file and it is not a secret. On the plain
(loopback) listener there is no certificate in the connection, so the host uses its
active pin anyway; a phone that never pinned has nothing to compare against.

**Test vector.** The APK must reproduce these values (computed with an independent
SHA-256 implementation and fixed in `crates/pin/src/sas.rs`):

| Input | Value |
|-------|-------|
| `host_pin`, base64url | `AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8` (bytes `00 01 02 … 1f`) |
| `device_public_key`, hex | `0411111111111111111111111111111111111111111111111111111111111111112222222222222222222222222222222222222222222222222222222222222222` (65 bytes) |
| expected SAS | `097665` |

Second vector, same pin, device key `04` `33`×32 `44`×32 (hex
`0433333333333333333333333333333333333333333333333333333333333333334444444444444444444444444444444444444444444444444444444444444444`):
expected SAS `827242`.

The host-side `add-device` CLI remains the manual path to the same result, and
is the way to grant anything, `READ_STATE` included, to a pending device.

## Trust store — IMPLEMENTED

- Path: `~/.config/excubia/devices.json` (override with `--devices`).
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
it verbatim (Desktop and APK copy it from `CORE4-RESULT.md`).

**Grant over HTTP — IMPLEMENTED (F1-núcleo).** A device with `ADMIN` can give a
pending device its first permissions with `POST /v1/devices/{device_id}/grant`
(see [`api.md`](api.md)). Only a device that holds no permission and is not revoked
can be granted this way:

- `admin` is refused (`400 admin_not_grantable`): ADMIN stays the host CLI's act;
- a device cannot grant itself (`400 cannot_grant_self`);
- a device that already holds any permission answers `409 not_pending`: widening
  it stays `host devices grant`;
- a revoked or unknown device answers `404`, as in revoke.

On success the host answers `200` with the device, appends a `granted` line to the
pairing audit (`via: admin`, `granted_by`, `permissions`) and posts the Alfred
notice above. The UI for granting (Desktop and APK) is not part of F1-núcleo and
remains pending.

- `host devices list` shows each device and its permissions, and under each pending device its verification string.
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

The LAN listener is off by default. `host --tls-bind <ip:port>` (repeatable) serves the
device routes over TLS 1.3 on a concrete address; a wildcard is refused, as
`--bind` refuses it. The hook routes and the internal MCP surface are never
mounted on this listener, so a LAN device cannot reach them.

`host pair-code --lan <ip:port>` (repeatable) prints the code and a pairing link:

```
excubia://pair?v=1&c=<code>&h=<pin>[&b=<backup pin>]&a=<ip:port>...&e=<unix seconds>
```
Lo anterior al cambio de nombre, `herdr-notify://`, se acepta al leer y nunca se emite; la carpeta vieja `~/.config/herdr-notify` se mueve sola a `~/.config/excubia` al arrancar.

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

## F2a — PAKE core (library) — IMPLEMENTED

Crate `crates/pake` (`hn-core-pake`): the cryptographic core of the short-code pairing in
`docs/rfc/pairing-security.md` §5. It exists and is tested. The host endpoints of *F2b* below now call
it; **the app does not** (no client, no mDNS). Pairing by link/code and the SAS flow above (F0/F1) are
unchanged.

Flow it supports (one run per code):

1. The host makes `Code::generate()` (8 characters of `A-Z` without `I`/`O` plus `2-9`, 40 bits, one attempt).
2. Both sides call `Session::start(role, code, host_id, device_pubkey)`: the **device is SPAKE2 side A**
   (`idA` = its public key), the **host side B** (`idB` = its id). Each sends the 33-byte message
   (side byte `A`/`B` + compressed Ed25519 point) and calls `finish(peer_message)`, which consumes the session.
3. Each derives a 32-byte `SessionKey`, equal only if the code and both identities matched.
4. Each sends `confirm_tag(role, tls_exporter, spki)` = HMAC-SHA256(key, `"excubia-pake-confirm-v1 "` ‖
   role byte (host 1, device 2) ‖ RFC 9266 `tls-exporter` (32 bytes) ‖ SHA-256 of the host SPKI) and checks the
   other's with `verify_confirm` (constant time). A relay holding two TLS sessions has two exporters, so the tags
   do not verify; a different host certificate changes the SPKI part.

F2b1/F2b2 done: the exporter (`PeerAddr::binding`, RFC 9266, `None` on loopback) and the in-memory code slot
(`PakeSlot`: one use, 180 s, never in `devices.json` or the `codes` store). F2b3 wired them to routes (below).

Rules for whoever builds the endpoint (F2b): a code is spent by its first run, right or wrong; a failed
confirmation burns it; only continue after both tags verify; the pin the device stores comes from the SPKI
it confirmed, not from any link.

Not what RFC 9382 specifies: the `spake2` 0.4.0 crate is python-spake2's Ed25519 SPAKE2 (other M/N constants,
SHA-256 transcript of `H(pw) ‖ H(idA) ‖ H(idB) ‖ X ‖ Y ‖ K`). Checked byte for byte against python-spake2 0.9
(`crates/pake/tests/vectors.rs`, `tests/interop/interop.py`). The crate also rejects the identity element and any
point outside the prime-order subgroup, which `spake2` alone accepts. Residual: `spake2` does not zeroize its
copy of the code or its scalars, and is not constant-time.

## F2b — PAKE endpoints (alpha, no client yet) — IMPLEMENTED in the host; no client (F2d)

Short-code pairing over the pinned TLS listener (`crates/api/src/routes/pake.rs`). Alpha: nothing in the app calls it.

1. **The operator** runs `host pair-code --pake [--port N] [--data-dir D]` (default port 8791). The CLI reads the
   host hook token from the data dir and calls `POST /v1/admin/pake-code` on `127.0.0.1:<port>`; it prints the code once
   (`ABCD-EFGH`) and «válido 180 s, un solo intento». The route exists **only on the loopback router**
   (`404` on the LAN listener), needs the **host** hook token (a per-run token is `403 host_token_required`; no token
   or a wrong one `401`), and answers `{"code": "ABCDEFGH", "expires_in": 180}` (`409 pake_unavailable` without a
   TLS identity). The code lives in the daemon's memory only; arming again replaces it and drops a pending session.
2. **`POST /v1/pair/pake/start`** `{"device_pubkey": b64, "message": b64}` → `{"message": b64, "tag": b64}`.
3. **`POST /v1/pair/pake/finish`** `{"tag": b64}` → the body of `POST /v1/pair` (`device_id`, `permissions: []`,
   `recovery_code`, `label: null`, `owner_id: ""`). Both routes are public like `/v1/pair`.

Formats: base64 standard with padding. `device_pubkey` = 65-byte SEC1 uncompressed P-256 point (as `/v1/pair`'s
`public_key`), also SPAKE2's idA; `message` = 33 bytes; `tag` = 32 bytes. `host_id` (idB) = lowercase hex of the SHA-256
SPKI hash of the active certificate (64 characters): the device computes it from the certificate it sees, so a relay
with another certificate gets another `host_id` and the PAKE does not match. The device is side A, the host side B.

What each tag binds (`HMAC(key, domain ‖ role ‖ exporter ‖ SPKI)`): the host's tag in the `start` answer, the exporter of the
connection that carried **that** `start`; the device's tag in `finish`, the exporter of the connection carrying **that**
`finish` (RFC 9266, label `EXPORTER-Channel-Binding`, empty context). `start` and `finish` may use different connections.
A relay with two TLS sessions makes the tags disagree. The device must verify the host's tag before sending its own.

Rules: a request without an exporter (loopback, or no TLS identity) is `403 pake_requires_tls` before the code is touched.
Each route takes one write-class budget from `pake:<ip>` (30/min, separate from `pair:<ip>`, shared by both routes).
Shape errors (JSON, base64, an invalid P-256 key, a message that is not 33 bytes) are `400 invalid_body` **before** the
code is taken and do not burn it. After that the code is spent, right or wrong: a bad point or side is `400
pake_invalid_message`, a second `start` is `410 no_live_code`. `start` keeps one pending session (60 s, one use, replaced by
a new `arm`); a wrong tag or expiry consumes it (`403 confirmation_failed`, `410 pake_session_gone`). On success the device
is **pending with no permissions**; the F1 SAS approval is still the only way to grant them.

Audit (`pairing-audit.jsonl`, no code, key, tag, exporter or device key): `pake_started` (code spent, host answered),
`pake_failed` (no live code, invalid message, no session, wrong tag) and `paired` with `via: "pake"`. Alfred's notice: only
`paired`, with the SAS string like any new device; `pake_started` and `pake_failed` post nothing.

Known and accepted: anyone on the LAN can burn the live code with a well-formed `start` (limited per IP); the code is then
re-armed. Not verified: no real phone, no real LAN, no client implementation (F2d).

## F2c — mDNS — IMPLEMENTED in the host; no client (F2d)

The host announces itself on the LAN so the app finds it without typing an IP (`crates/host/src/mdns.rs`, `mdns-sd =0.21.7`).

- **Service** `_excubia-pair._tcp.local.`, instance `excubia-<8 random hex of this run>` (never the user's or the PC's name),
  port = the LAN TLS port. **TXT**: `v=1` and nothing else.
- **When**: only while `PakeSlot::is_live()` (an armed code that has not expired, or a pending session that has not): checked
  every 1 s; it registers on the way in and withdraws, with a goodbye packet (RFC 6762 §10.1), on the way out (expiry, burn,
  `clear`, end of the ceremony) and when the daemon stops. No `--tls-bind`, or the first one did not bind: nothing is announced.
- **Where**: only the interface of the first `--tls-bind` address (the service and the daemon are both restricted to it).
- **It does NOT contain** the code, the SPKI, any device id or host id, nor the PC or user name. It is not a secret and not an
  authentication: the PAKE protects the pairing whether or not anyone sees the announcement.
- **TTL**: the `mdns-sd` defaults (host 120 s, PTR/TXT 4500 s). If the process dies without a goodbye the announcement can
  linger up to ~75 min on other devices; the Master accepted this. The TTL setters are not public in this version.
- **Failure**: if `mdns-sd` cannot start or register, the host logs one line without secrets and pairing by IP and PAKE goes on.
  Errors the daemon reports later from its own thread are not logged.
- **Windows firewall** asks per program path: allow `host.exe` from the path that is launched (UDP 5353 inbound), or the
  queries of the app may not reach it. Not verified on a real LAN.

## F3a — v2 link, the QR without an address — IMPLEMENTED in the host; no client yet (F3b-F3d)

- **Format**: `excubia://pair?v=2&c=<8 chars>&h=<host pin, base64url 43>&e=<unix expiry>`. `c` is the PAKE code (alphabet of
  `hn-core-pake`), `h` the same SPKI `Fingerprint` as in v1 (the device hex-encodes it for the PAKE `host_id`), `e` when the code dies.
  About 90 bytes. No `a`, no `b`: the device finds the host by mDNS (F2c) and must see `h` before it sends anything.
- **Parser**: `hn_core_pin::PakeLink::parse` (and `to_uri`, `is_expired(now)`). It demands `v=2` and refuses, with a typed
  `PinError`, `a`/`b`, unknown or repeated fields, a code that is not 8 characters of the alphabet, a bad `h` and a non-numeric `e`.
  It does not check the expiry: the caller does, with its own clock. `PairingLink::parse` (v1) is unchanged and refuses `v=2`.
- **Source**: `POST /v1/admin/pake-code` (loopback, host token only, `Cache-Control: no-store`) now answers
  `{code, expires_in, expires_at, uri}`; `uri` is built with the host's pin. `host pair-code --pake --uri` also prints
  `pairing uri: <uri>`; `--uri` without `--pake` is a usage error. A daemon from before F3a sends no `uri`; the CLI says so.
- **Secret**: the URI holds the live code, so it is as secret as the code: never logged, audited or stored (tests scan the data
  dir). It is for whoever shows it on screen. TTL stays 180 s. The v1 QR with addresses (`--lan`) stays for now.
- **Compatibility**: an app from before F3a that sees `h` hands the link to `PairingLink::parse`, gets
  `UnsupportedLinkVersion("2")` and fails closed ("update the app"). What authorizes the device is still the PAKE plus the SAS
  approval of F1; the pin in the link only makes the device refuse a host that is not the one on screen.

## Implementation status

| Capability | Status |
|------------|--------|
| Device identity (P-256 key in `flutter_secure_storage`) | IMPLEMENTED |
| Host-side `add-device` CLI pairing | IMPLEMENTED |
| `POST /v1/pair` (code / recovery / tailnet owner) | IMPLEMENTED (Fase 2(c)) |
| Pairing codes (4 bytes, 180 s, single use) | IMPLEMENTED |
| Recovery codes (re-key, rotate, SHA-256 hash only) | IMPLEMENTED |
| Tailnet auto-approval (`tailscale whois`, same owner) | IMPLEMENTED |
| Trust store (atomic writes, fail closed, live reload) | IMPLEMENTED |
| Pending device on pairing (no permission until the host grants one, D3) | IMPLEMENTED (CORE11) |
| LAN TLS listener, pinned pairing link, pin rotation (`tls prepare` / `activate`) | IMPLEMENTED (CORE11) — see *LAN TLS pairing link* |
| Device management (`GET /v1/devices`, `POST /v1/devices/{id}/revoke`) behind `ADMIN` | IMPLEMENTED (Fase 2(c)) |
| Tailnet peer list (`GET /v1/peers`, `503` when unavailable) | IMPLEMENTED |
| HTTP grant endpoint, pending devices only, never `admin` (`POST /v1/devices/{id}/grant`) | IMPLEMENTED (F1-núcleo; UI pending). Widening a device that already has access stays a host-side CLI act by design |
| Verification string (SAS) on the pending device, in `devices list`, `GET /v1/devices` and the pairing notice | IMPLEMENTED (F1-núcleo; the APK reproduces the vector) |
| PAKE core (`crates/pake`, SPAKE2 + TLS-bound key confirmation) | IMPLEMENTED (F2a: library, see *F2a*) |
| PAKE endpoints `POST /v1/pair/pake/start` and `/finish`, loopback admin route, `host pair-code --pake` | IMPLEMENTED in the host (F2b3, alpha); no client (F2d), see *F2b* |
| mDNS announcement `_excubia-pair._tcp` only while a PAKE code is alive | IMPLEMENTED in the host (F2c); no client (F2d), see *F2c* |
| v2 pairing link `excubia://pair?v=2&c&h&e` (QR without an address), `pake-code` answers `uri` and `expires_at`, `pair-code --pake --uri` | IMPLEMENTED in the host (F3a); no client yet, see *F3a* |
| `APPROVE` permission endpoints | IMPLEMENTED (CORE8) — `POST /v1/chat/approvals/{id}/decision`, the UAC decision |
