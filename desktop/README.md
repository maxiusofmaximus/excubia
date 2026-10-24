# herdr-notify

Push notifications and a secure control plane for
[herdr](https://github.com/herdrdev/herdr) coding agents.

When an agent running in herdr needs you — it hit an approval dialog, or it
finished a task you asked for — your phone buzzes, and tapping it takes you
straight to the answer. No terminal hunting, no pane-by-pane archaeology.

## Status

| Phase | What exists | Gate |
|-------|-------------|------|
| **F1** herdr client | Async client, gap-free bootstrap, live stream | 32 unit tests + 3 live tests green |
| **F2** real-time state | `events.subscribe`, stream rotation, authoritative reducer | idempotent under replay; rolls up per workspace |
| F3 Firebase | FCM HTTP v1 sender, push policy, dedup/cooldown | pending |
| F4 control plane | HTTPS API + SSE, device tokens, Tailscale | pending |
| F5 Android app | Compose app, FCM receiver, notifications | pending |
| F6 GUI | Triage grid, agent detail, design system | pending |
| F7 actions | Approve / respond via `agent.send_keys` and `agent.prompt` | pending |
| F8 E2E | Deterministic ADB-driven tests on a real phone | pending |
| F9 security | Audit with `cloudflare/security-audit-skill` | pending |

## Architecture

```
herdr server  ──NDJSON──▶  gateway (Python, runs beside herdr)
                                │
                                ├─ herdr client     subscribe + request/response
                                ├─ state store      authoritative reduced session
                                ├─ attention ranker blocked ▸ done ▸ working ▸ idle
                                ├─ push policy      dedup, cooldown, budget
                                └─ HTTP API         127.0.0.1 only
                                       │
                        ┌──────────────┴───────────────┐
                    FCM HTTP v1                  Tailscale serve
                        │                              │
                        └──────────▶ 📱 Android ◀──────┘
                                   push + live SSE
```

**No port is exposed to the internet.** The gateway binds loopback; Tailscale
publishes it over TLS inside the tailnet. Interactive SSH, when wanted, is
Tailscale SSH — Ed25519 keys, no `sshd` install, no forwarded port.

## Layout

```
gateway/     Python 3.13 · herdr client, state store, (soon) push + API
android/     Kotlin · Jetpack Compose app
e2e/         Deterministic ADB-driven end-to-end scenarios
tooling/     Firebase bootstrap, wireless-device pairing
docs/        Protocol notes, threat model, setup guides
security/    Audit artefacts (coverage ledger, findings, report)
```

## Quick start

```bash
cd gateway
uv venv --python 3.13 .venv
uv pip install --python .venv/Scripts/python.exe -e ".[dev]"

# Does the herdr endpoint resolve, and is the server answering?
.venv/Scripts/python.exe -m herdr_notify.cli doctor --ping

# What does herdr look like right now?
.venv/Scripts/python.exe -m herdr_notify.cli dump
PYTHONPATH=src .venv/Scripts/python.exe -m herdr_notify.cli dump --follow
```

## Testing

```bash
cd gateway
.venv/Scripts/python.exe -m pytest              # 32 unit tests, hermetic
.venv/Scripts/python.exe -m pytest -m integration   # 3 tests, needs a running herdr
.venv/Scripts/python.exe -m ruff check src tests
.venv/Scripts/python.exe -m mypy src
```

Unit tests drive a scriptable in-process fake of herdr's API rather than a live
session, because the behaviour worth testing — subscribe-before-snapshot,
rotation without losing an event, idempotence under replay — is timing behaviour,
and mutating a real coding session to exercise it is not an acceptable trade.

## Read this before touching the herdr client

[`docs/herdr-integration.md`](docs/herdr-integration.md) documents the protocol
facts that are **not** in herdr's prose docs and that each cost real debugging
time: the endpoint is a named pipe and not the `.sock` file, there is exactly one
request per connection, subscription sets are frozen at subscribe time,
`session.snapshot` nests under `result.snapshot`, and the Windows CRT
read/write position plus `CloseHandle` deadlock are both real and both fail
confusingly.

## Seguridad

Sin token estático: cada dispositivo es una clave pública P-256 que el host
registra y firma cada petición; el host puede revocarla y los permisos son por
dispositivo. El gateway rechaza direcciones comodín (`0.0.0.0`, `::`) y corre
solo dentro de Tailscale (más loopback). Ver `docs/contracts/auth.md`.

Configuración local (no versionada): copia `android/local.properties` con
`gatewayUrl=http://<tu-ip-tailscale>:8788` y `tailnetHosts=<tu-ip-tailscale>`.
Sin ese fichero la app compila con placeholders inocuos.

## Security posture

The gateway is the crown jewel: it holds a socket that can drive any coding agent
on the machine. Design rules already in place:

- The client may only ask for **typed actions**. It can never name a herdr
  method, so there is no arbitrary-invocation surface.
- The resolved herdr endpoint is pinned inside herdr's config directory, so a
  tampered `HERDR_SOCKET_PATH` cannot redirect the control plane.
- Event payloads are treated as untrusted; an unrecognised status or event name
  is dropped rather than stored.
- Secrets are gitignored, and the FCM service account is referenced by path, not
  embedded.

Formal audit: phase F9.

## Desktop client (Tauri)

The desktop app lives in this worktree: a Tauri 2 shell that hosts the Rust
`core` as a child process (sidecar). Build both sidecars with
`scripts\build-host.ps1` (the daemon and `hn-mcp`, which Alfred needs) and the
app with `pnpm dlx @tauri-apps/cli@latest build` or, for a development run,
`cargo build -p herdr-notify-desktop`. Data goes to
`%APPDATA%\dev.herdrnotify.desktop\core` and the host log to
`%LOCALAPPDATA%\dev.herdrnotify.desktop\logs\host.log`.

The first screen is the chat with Alfred (three zones: history, conversation
and a collapsible team panel). **Equipo** is one tap away and keeps the agent
list, detail, respond, stop and new-conversation actions. Chat reads and writes
go to the real host; notifications still come from the agent state stream.
`cargo test --workspace` covers the reducer, the error cards and a real-host
chat flow with the simulated CLI.

### Emparejar con un host por enlace (CORE11B)

Pega en la pantalla de emparejamiento el enlace `herdr-notify://pair?…` que
imprime `host pair-code --lan`. Trae la huella SPKI del certificado del host
(`h=`, y `b=` si hay respaldo): el Desktop solo completa el emparejamiento si
el host presenta un certificado que coincide, por TLS 1.3 con pin. Un host
distinto muestra *La huella del host no coincide. No te conectes a esta red.*;
un enlace caducado o de versión desconocida se rechaza antes de conectar. Los
pines se guardan en `config.json` (`gateway_pins`); la clave privada sigue en
el almacén de credenciales.

Sin `h=` la conexión es HTTP y solo se acepta hacia loopback o la tailnet
(`*.ts.net`); fuera de loopback la pantalla avisa de que no va cifrada.

**Mostrar código QR** dibuja el enlace LAN en Rust (crate `qrcode`, MIT/Apache-2.0)
sin servicios externos. El interruptor de red local está desactivado por
defecto; con él activo, el host escucha en `IP-privada:8443` solo para las
rutas de dispositivo. Capturas: `evidence/core11b/`.

Agent hooks (Claude Code, opencode) report their own state to the host with
`HN_PANE_ID` / `HN_HOOK_URL` / `HN_HOOK_TOKEN`. The token is printed by
`host hook-token` and no secret is copied here. Full configuration and
examples: [`core/docs/core/HOOKS.md`](../core/docs/core/HOOKS.md).
