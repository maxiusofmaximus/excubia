# DESKTOP8 — worklog

Order of work, with the checkpoint kept in the tree after each step.

1. **Contract read** (`core/docs/contracts/api.md` §`/v1/chat`, `state.md`,
   `errors.md`, `orchestrator-chat.md` §Hito 1). No endpoint or field invented.
2. **Core client** (`core/src/chat/`): wire models, signed calls, typed
   `ChatFailure`, section patch (`Keep`/`Clear`/`Set`) and id validation.
3. **Reducer** (`core/src/chat_feed.rs`): one `BTreeMap<seq, entry>`; messages,
   cards and unknown kinds merge without duplicates.
4. **SSE**: `chat.*` frames reach `EventSink::chat` and never trigger a
   `/v1/state` refresh.
5. **Tauri commands** (`chat_commands.rs`) return reduced snapshots; the
   webview chooses a card by `kind`, not by HTTP status.
6. **UI**: chat-first `view-chat` with history, conversation and collapsible
   Equipo panel; `Equipo` keeps the DESKTOP7 screens. Light and dark themes,
   32 px controls, focus rings and `aria-label` on icon buttons.
7. **Tests**: reducer, error mapping (409/422/429/403), stream frame, and
   `chat_integration.rs` against the real host with a patched test adapter.
8. **Evidence**: `evidence/desktop8/` (local host + simulated CLI for the happy
   path; deterministic fake host for 403/409/422/429/offline). No real data.

## Reproduce

```powershell
scripts\build-host.ps1 -Profile debug
cargo build -p herdr-notify-desktop
cargo fmt --all --check
cargo clippy --workspace --all-targets --features herdr-desktop-core/os-keyring -- -D warnings
cargo test --workspace
```

`tooling/fake-claude.rs` is a test-only adapter: it answers the Claude version
probe and translates the Claude MCP config into the shape `fake-cli` reads.
`tooling/fake-chat-server.mjs` serves the deterministic error screens; it is
not part of the app.

## Evidence map

| File | Scenario |
|---|---|
| `01-inicio-vacio-*.png` | first screen, no conversations |
| `02-conversacion-3-mensajes-*.png` | three turns answered by the host |
| `03-historial-pin-seccion-*.png` | pinned rows and a section |
| `04-busqueda-*.png` | server-side search |
| `05-sin-conexion-*.png` | offline with the last readable messages |
| `06-seat-unavailable-*.png` | 409 `seat_unavailable` |
| `07-agent-busy-*.png` | 429 + `Retry-After` |
| `08-turno-fallido-*.png` | real turn without `chat.reply` |
| `09-equipo-*.png` | secondary Equipo view |
| `10-error-403-*.png` | missing `RESPOND` + canonical grant command |
| `11-error-422-*.png` | validation error |

Each scenario has dark and light captures.
