# Excubia

Tu equipo de agentes de IA, a la vista y bajo tu control. Excubia aloja
agentes de línea de comandos (Claude Code, opencode y otros), te los muestra
en una app de escritorio y otra de móvil, y te pide el visto bueno antes de
que hagan nada delicado.

Alfred, el director técnico, coordina a un equipo contratado por oficio:
programación, vídeo, diseño, audio, documentos y más. Hablas con él en un
chat, y te avisa cuando necesita tu aprobación o termina lo que le pediste.

## Estado

Hitos y verificaciones: [`docs/roadmap.md`](docs/roadmap.md).

| Crate | Qué hace |
|-------|----------|
| `excubia-core-host` (bin `host`) | Hospeda agentes CLI: un pty por agente, detector y API local |
| `excubia-core-api` | API local HTTP + SSE con peticiones firmadas (contratos en `docs/contracts/`) |
| `excubia-core-auth` | Emparejamiento, verificación de firmas P-256 / Ed25519 y autorización |
| `excubia-core-state` | Estado autoritativo de los agentes |
| `excubia-core-detect` | Estado de cada agente a partir de manifiestos |
| `excubia-core-pty` | Pseudoterminal por agente |
| `excubia-core-pin` | Identidad TLS del host y la regla de pin que verifican los dispositivos |
| `excubia-core-chat`, `excubia-core-mail`, `excubia-core-asr`, `excubia-core-seat`, `excubia-core-media`, `excubia-core-persist` | Chat persistente, borradores de Microsoft 365, voz a texto local, adaptador de CLIs reales, validación de imágenes, escritura atómica y secretos del Administrador de credenciales |

## Architecture

```
agentes CLI ──pty──▶ excubia-core-host ──detect──▶ excubia-core-state
                          │
          ┌───────────────┴────────────────┐
     excubia-core-api
  HTTP + SSE firmado
          │
     clientes (app móvil y escritorio)
```

La identidad TLS del host y su pin viven en `excubia-core-pin`; el rechazo de
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
cargo run -p excubia-core-host --bin host -- --help   # imprime el uso
```

## Testing

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Los vectores dorados de firma viven en `docs/contracts/vectors/signing_vectors.json`
y los comprueba `excubia-core-test-support`.

JavaScript del worktree: pnpm o vp; no npm.

## Integración con herdr (retirada)

Excubia ya no depende de herdr: el host hospeda los agentes él mismo.
[`docs/herdr-integration.md`](docs/herdr-integration.md) se conserva solo como
registro histórico del cliente que existió.

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
