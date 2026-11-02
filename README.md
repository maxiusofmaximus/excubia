# herdr-notify

Live agent alerts and a secure control plane for
[herdr](https://github.com/herdrdev/herdr) coding agents.

When an agent running in herdr needs you — it hit an approval dialog, or it
finished a task you asked for — your phone buzzes, and tapping it takes you
straight to the answer. No terminal hunting, no pane-by-pane archaeology.

## Estado

Hitos y verificaciones: [`docs/roadmap.md`](docs/roadmap.md).

| Crate | Qué hace |
|-------|----------|
| `hn-core-host` (bin `host`) | Hospeda agentes CLI: un pty por agente, detector y API local |
| `hn-core-api` | API local HTTP + SSE con peticiones firmadas (contratos en `docs/contracts/`) |
| `hn-core-auth` | Emparejamiento, verificación de firmas P-256 / Ed25519 y autorización |
| `hn-core-state` | Estado autoritativo de los agentes |
| `hn-core-detect` | Estado de cada agente a partir de manifiestos |
| `hn-core-pty` | Pseudoterminal por agente |
| `hn-core-pin` | Identidad TLS del host y la regla de pin que verifican los dispositivos |
| `hn-core-chat`, `hn-core-mail`, `hn-core-asr`, `hn-core-seat`, `hn-core-media`, `hn-core-persist` | Chat persistente, borradores de Microsoft 365, voz a texto local, adaptador de CLIs reales, validación de imágenes, escritura atómica y secretos del Administrador de credenciales |

## Architecture

```
agentes CLI ──pty──▶ hn-core-host ──detect──▶ hn-core-state
                          │
          ┌───────────────┴────────────────┐
     hn-core-api
  HTTP + SSE firmado
          │
     clientes (app móvil y escritorio)
```

La identidad TLS del host y su pin viven en `hn-core-pin`; el rechazo de
direcciones comodín en `--bind` está en `crates/host/src/cli/run.rs`.

## Layout

```
crates/      núcleo Rust: workspace de 15 crates
tooling/     scripts PowerShell y plugin de UAC para opencode
docs/        contratos (docs/contracts/), RFC, roadmap, notas del núcleo
```

## Quick start

```bash
cargo build --workspace
cargo run -p hn-core-host --bin host -- --help   # imprime el uso
```

## Testing

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Los vectores dorados de firma viven en `docs/contracts/vectors/signing_vectors.json`
y los comprueba `hn-core-test-support`.

JavaScript del worktree: pnpm o vp; no npm.

## Read this before touching the herdr client

[`docs/herdr-integration.md`](docs/herdr-integration.md) documents the protocol
facts that are **not** in herdr's prose docs and that each cost real debugging
time: the endpoint is a named pipe and not the `.sock` file, there is exactly one
request per connection, subscription sets are frozen at subscribe time,
`session.snapshot` nests under `result.snapshot`, and the Windows CRT
read/write position plus `CloseHandle` deadlock are both real and both fail
confusingly.

## Seguridad

Sin token estático: cada dispositivo se identifica con una clave pública
(P-256 o Ed25519, `crates/auth/src/curve.rs`) y firma sus peticiones; el host
guarda solo el hash del código de recuperación (`crates/auth/src/store/file.rs`).
`--bind` rechaza direcciones comodín (`0.0.0.0`, `::`)
(`crates/host/src/cli/run.rs`). Contratos: `docs/contracts/auth.md`.

## Security posture

El host supervisa los ptys de los agentes, así que es la parte sensible:

- Las acciones sobre agentes son un trait tipado, `AgentActions`
  (`crates/api/src/lib.rs`), implementado por el host (`crates/host/src/actions.rs`).
- Las imágenes subidas se validan por su cabecera real y se recodifican a WebP,
  solo píxeles (`crates/media`).
- Los dispositivos verifican la identidad TLS del host por pin (`crates/pin`).
- Secretos fuera de git: `service-account*.json` y `google-services.json`
  (`.gitignore`).

Revisión de seguridad previa: [`docs/security-review.md`](docs/security-review.md)
(su alcance original fue la implementación Python retirada).
