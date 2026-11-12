# Excubia para escritorio

Cliente de escritorio de Excubia (Tauri 2): chat con Alfred, tu equipo de
agentes en vivo, calendario, notas, contactos, avisos y dispositivos. Lleva el
host de Excubia como proceso hijo, así que funciona sin instalar nada más.

## Arquitectura

```
agentes CLI ──pty──▶ host (Rust)        núcleo de Excubia
                       │
                       └─ API HTTPS firmada   loopback; LAN con TLS fijado
                              │
                ┌─────────────┴──────────────┐
          Escritorio (Tauri)          App móvil (Flutter)
```

**No se expone ningún puerto a internet.** El host escucha en loopback por
defecto y sirve a la LAN solo si se le pide, siempre con TLS fijado.

## Estructura

```
core/            Rust · cliente del host, sin interfaz (crate excubia_desktop_core)
src-tauri/       Tauri 2 · shell de escritorio que ejecuta el host como sidecar
ui/              HTML/JS · pantallas y sus modelos puros
tooling/         pruebas node de los modelos, host y chat simulados
tests/fixtures/  vectores dorados compartidos (firma de peticiones)
docs/            contratos, hoja de ruta, credenciales, revisión de seguridad
```

## Pruebas

```bash
cargo test --workspace                 # core y Tauri, contra un host simulado
node --test tooling/*.test.mjs         # modelos y pantallas de la interfaz
```


## Seguridad

Sin token estático: cada dispositivo es una clave pública P-256 que el host
registra y firma cada petición; el host puede revocarla y los permisos son por
dispositivo. El host rechaza direcciones comodín (`0.0.0.0`, `::`) y corre
solo en loopback, la LAN o la tailnet. Ver `docs/contracts/auth.md`.

## Postura de seguridad

El host es la pieza sensible: puede manejar a cualquier agente de la máquina.

- El cliente solo puede pedir **acciones tipadas**; no hay forma de invocar
  métodos arbitrarios.
- Los eventos de los agentes se tratan como no confiables: un estado o evento
  desconocido se descarta.
- Los secretos están fuera de git.

## Desktop client (Tauri)

The desktop app lives in this worktree: a Tauri 2 shell that hosts the Rust
`core` as a child process (sidecar). Build both sidecars with
`scripts\build-host.ps1` (the daemon and `excubia-mcp`, which Alfred needs) and the
app with `pnpm dlx @tauri-apps/cli@latest build` or, for a development run,
`cargo build -p excubia-desktop`. Data goes to
`%APPDATA%\dev.excubia.desktop\core` and the host log to
`%LOCALAPPDATA%\dev.excubia.desktop\logs\host.log`. The folders of the old
name (`dev.herdrnotify.desktop`) are moved there on the first start.

The first screen is the chat with Alfred (three zones: history, conversation
and a collapsible team panel). **Equipo** is one tap away and keeps the agent
list, detail, respond, stop and new-conversation actions. Chat reads and writes
go to the real host; notifications still come from the agent state stream.
`cargo test --workspace` covers the reducer, the error cards and a real-host
chat flow with the simulated CLI.

### Emparejar con un host por enlace (CORE11B)

Pega en la pantalla de emparejamiento el enlace `excubia://pair?…` que
imprime `host pair-code --lan`. Trae la huella SPKI del certificado del host
(`h=`, y `b=` si hay respaldo): el Desktop solo completa el emparejamiento si
el host presenta un certificado que coincide, por TLS 1.3 con pin. Un host
distinto muestra *La huella del host no coincide. No te conectes a esta red.*;
un enlace caducado o de versión desconocida se rechaza antes de conectar. Los
pines se guardan en `config.json` (`host_pins`); la clave privada sigue en
el almacén de credenciales.

Sin `h=` la conexión es HTTP y solo se acepta hacia loopback o la tailnet
(`*.ts.net`); fuera de loopback la pantalla avisa de que no va cifrada.

**Mostrar código QR** dibuja el enlace LAN en Rust (crate `qrcode`, MIT/Apache-2.0)
sin servicios externos. El interruptor de red local está desactivado por
defecto; con él activo, el host escucha en `IP-privada:8443` solo para las
rutas de dispositivo. Capturas: `evidence/core11b/`.

Agent hooks (Claude Code, opencode) report their own state to the host with
`EXCUBIA_PANE_ID` / `EXCUBIA_HOOK_URL` / `EXCUBIA_HOOK_TOKEN`. The token is printed by
`host hook-token` and no secret is copied here. Full configuration and
examples: [`core/docs/core/HOOKS.md`](../core/docs/core/HOOKS.md).
