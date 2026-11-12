# Excubia

Tu equipo de agentes de IA, a la vista y bajo tu control. Excubia aloja en tu
PC agentes de línea de comandos (Claude Code, Codex, opencode y otros), te los
muestra en una app de escritorio y otra de móvil, y te pide el visto bueno
antes de que hagan nada delicado.

Alfred, el director técnico, coordina a un equipo contratado por oficio:
programación, vídeo, diseño, audio, documentos y más. Hablas con él en un
chat, y te avisa cuando necesita tu aprobación o termina lo que le pediste.

## Instalar

### Escritorio (Windows 10/11, 64 bits)

En PowerShell:

```powershell
irm https://raw.githubusercontent.com/maxiusofmaximus/excubia/main/desktop/scripts/install.ps1 | iex
```

Descarga la última versión, comprueba su huella SHA-256 y la instala para tu
usuario. Repite el mismo comando para actualizar. Si prefieres el instalador,
está en [Releases](https://github.com/maxiusofmaximus/excubia/releases)
(`Excubia-X.Y.Z-setup.exe`, etiquetas `desktop-v…`).

El escritorio trae dentro el host, el programa que aloja a los agentes: no hay
nada más que instalar. Los agentes sí van aparte: Excubia usa los que ya tengas
en el PC (`claude`, `codex`, `commandcode`, `gemini` u `opencode`).

### Móvil (Android)

Descarga el APK de la [última versión](https://github.com/maxiusofmaximus/excubia/releases/latest)
e instálalo. Después empareja el móvil con tu PC: en el escritorio, abre
**Dispositivos**, escanea el QR con el móvil y comprueba que el código corto
coincide en las dos pantallas.

Paso a paso: [guía de usuario](https://maxiusofmaximus.github.io/excubia/guide/).

## Cómo funciona

```
agentes CLI ──pty──▶ host (Rust, en tu PC)
                       │
                       └─ API HTTPS firmada   loopback; LAN con TLS fijado
                              │
                ┌─────────────┴──────────────┐
          Escritorio (Tauri)          App móvil (Flutter)
```

- **Ningún puerto abierto a internet.** El host escucha en tu PC y, si lo
  pides, en tu red local, siempre con TLS y la huella del host fijada en cada
  dispositivo.
- **Sin contraseñas ni tokens compartidos.** Cada dispositivo tiene su propia
  clave y firma cada petición; puedes revocar uno sin tocar los demás.
- **Tú apruebas lo delicado.** Las acciones sensibles de un agente esperan tu
  visto bueno en el escritorio o en el móvil.

## Documentación

- [Guía de usuario](https://maxiusofmaximus.github.io/excubia/guide/): instalar, emparejar, chat con Alfred, aprobaciones, problemas frecuentes.
- [Documentación técnica](https://maxiusofmaximus.github.io/excubia/docs/) y [referencia de API](https://maxiusofmaximus.github.io/excubia/api/).
- Contratos de la API del host: [`docs/contracts/`](docs/contracts/).

## Código

| Carpeta | Qué es |
|---------|--------|
| [`core/`](core/) | Núcleo en Rust: el host (`excubia-host`) y sus crates `excubia-core-*` |
| [`desktop/`](desktop/) | App de escritorio (Tauri 2) |
| [`apk/`](apk/) | App de Android (Flutter, con núcleo Rust) |
| [`docs/`](docs/) | Guía, documentación técnica y contratos |

Cada carpeta tiene su README con cómo compilar y probar. Para el núcleo:

```bash
cd core
cargo build --workspace
cargo run -p excubia-core-host --bin excubia-host -- --help
cargo test --workspace
```

## Problemas y sugerencias

Abre una [issue](https://github.com/maxiusofmaximus/excubia/issues).

## Licencia

[MIT](LICENSE).
