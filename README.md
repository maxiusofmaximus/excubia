# herdr-notify

Control and notifications for coding agents from your phone and desktop: see
what your agents are doing, approve or deny the actions they ask for, and talk
to the orchestrator, from anywhere.

**The product is the Flutter app in [`apk/mobile`](apk/mobile)** (package
`dev.herdrnotify.app`). The earlier Kotlin app is retired and kept for
reference in [`apk/_legacy-kotlin-v1`](apk/_legacy-kotlin-v1).

## Layout

| Directory | What it is |
|-----------|------------|
| [`core/`](core) | Rust host: owns the agent sessions, the client API (HTTPS/SSE), device pairing and approvals. TLS 1.3 with a pinned host key for LAN use. |
| [`apk/`](apk) | Android app (Flutter UI + Rust client core). |
| [`desktop/`](desktop) | Desktop app (Tauri). |
| `gateway/` | Original Python gateway for [herdr](https://github.com/herdrdev/herdr). Superseded by `core/`; kept until the migration ends. |
| `docs/` | Documents from the previous (Kotlin-era) release. Current contracts, RFCs and the security review live next to their component (`core/docs/`, `apk/docs/`); API reference docs are generated from the code. |
| `tooling/` | Scripts shared by the project. |

Each component builds on its own (`cargo` in `core/` and `desktop/`,
`flutter` in `apk/mobile`) and has its own README. Components reference each
other by relative path (for example the shared pin-verification crate
`core/crates/pin`), so keep the layout as is.

## Pairing and transport

A phone pairs with a host by scanning a QR code (or pasting the link). On a
local network the link carries the host's key fingerprint, and the client only
talks TLS 1.3 to a host whose key matches it. The transport never
authenticates a device: device keys do, and the host confirms every new device
before it gets any permission. Tailscale and similar VPNs remain optional.

## Security

Never commit signing keys, `keystore.properties`, `google-services.json` or
service accounts; `.gitignore` is the last line of defence, not the first.
Report vulnerabilities privately through GitHub security advisories.

## License

See [`LICENSE`](LICENSE) (and `core/NOTICE` for third-party notices).
