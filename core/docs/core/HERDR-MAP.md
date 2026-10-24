# HERDR-MAP — herdr 0.9.5, inventory and reuse plan for the 100 % Rust core

Read-only clone: `D:\Documentos\Projects\herdr-notify-wt\_src\herdr` (0.9.5 plus
Unreleased; one squashed commit, 2026-10-16). ~303 k lines of Rust (`src/` 270 k,
`crates/ghostty-vt` 12 k, `vendor/` 313 k of third-party sources). Licence
Apache-2.0. Verdicts: **reuse** / **adapt** / **rewrite** / **discard**.
Key reference docs: `docs/next/website/src/content/docs/{socket-api,agents,
add-herdr-support}.mdx`, `docs/next/api/herdr-api.schema.json`,
`skills/herdr/SKILL.md`.

## Module inventory

| Module | Lines | What it does | Native deps | Verdict |
|---|---|---|---|---|
| `src/pty` | 3 099 (`actor.rs` 842, `actor/unix.rs` 1 878, `fd.rs` 241) | Actor around `portable-pty`: spawn, resize, write queue, read loop. Windows: 3 threads + tokio forwarder; Unix: 1 poll thread | none (pure Rust) | **adapt** |
| `vendor/portable-pty` | patched `=0.9.0` | 4 patches: app-local `conpty.dll` pinned by SHA-256, `cmd /d /c` tail, malformed-env rejection, `search_path` hardening | none | **adapt** (stock 0.9.0 suffices for the prototype) |
| `src/detect` | 6 798 (`mod.rs` 2 182, `manifest.rs` 1 536, `manifest_update.rs` 1 012, `manifests/*.toml` 2 707) | Agent identification from the process tree; state from TOML rules over screen/OSC; agent-side hooks | none | **adapt** |
| `src/pane*` | 11 792 (`pane.rs` 6 843, `agent_detection.rs` 707, `osc.rs` 1 477) | Pane runtime: feeds the emulator, detection task, send text/keys, read text | ratatui (`Rect`/`Frame` in `pane.rs`, `terminal.rs`) | **rewrite** |
| `src/terminal` + `crates/ghostty-vt` | 8 367 + 11 772 | `ghostty-vt` is an FFI binding over vendored **libghostty-vt (Zig)**; `build.rs` runs `zig build` (Zig 0.16.0 required). Read APIs: `read_text_screen`, `screen_text_rows_range`, `detection_text` | Zig 0.16.0 (free, **not installed here**) | **discard** for now; viable later if Zig is accepted as a build dependency |
| `src/api` + `src/protocol` + `src/server` | 8 789 + 8 222 + 28 131 | NDJSON socket API (one request per connection; named pipe / unix socket), schemars schema, in-memory `EventHub` (512 retention), TUI serving | none | **adapt ideas** (schema types, `EventHub`, `state_change_seq`); transport **rewrite** (HTTP+SSE) |
| `src/persist` + `src/session.rs` | 4 820 + 1 027 | JSON pretty, tmp+rename+fsync, 5 s debounce, `SessionSnapshot` v3, pane/agent-resume restore; events are not persisted | none | **adapt later** (next phase) |
| `src/platform` | 15 814 (`windows.rs` 5 606) | Process tree (Toolhelp32, `NtQueryInformationProcess`, PEB marker), job objects, ConPTY fallback encoder, signals | win32 crates only | **adapt selectively** |
| `src/ui*`, `kitty_graphics*`, `sound.rs`, `remote*`, `plugin_*`, `input/` (TUI part) | ~25 k | ratatui UI, kitty images, sounds, SSH remote, plugins, keybindings | ratatui, crossterm | **discard** |

## Evidence: detection

- **Agent**: foreground-job process basename → alias table (`claude`/`claude-code`,
  `opencode`/`opencode2`/`open-code`, `cursor-agent`, `ghcs`, …), wrapper
  unwrapping (`node`, `bun`, `python`, `sh`, `cmd`, `powershell`), known
  `node_modules` paths, and on Windows a PEB marker `HERDR_PANE_RUNTIME_ID`.
  24 agents: pi, claude, codex, gemini, cursor, devin, antigravity, cline, omp,
  mastracode, opencode, copilot, kimi, kiro, droid, amp, grok, hermes, kilo,
  qodercli, qwen, letta, maki, muse.
- **Adding an agent**: Rust enum entry + label + executable aliases + bundled
  TOML manifest (+ optional hook installer and resume plan). Manifests can be
  overridden per machine (`~/.config/herdr/agent-detection/<agent>.toml`) or
  refreshed from a remote catalogue.
- **State**: rules over regions (`whole_recent`, `after_last_horizontal_rule`,
  `prompt_box_body`, `bottom_non_empty_lines(n)`, `osc_title`, `osc_progress`, …)
  with gates `contains` (case-insensitive), `regex`, `line_regex`,
  `all`/`any`/`not`, evaluated by descending `priority`; first match wins.
  Examples: claude blocked = `esc to cancel` + `enter to confirm`; claude idle =
  `^\s*❯`; codex blocked = OSC title `Action Required`; opencode blocked =
  `△ Permission required`.
- **Hooks**: agent-side scripts call `pane.report_agent` (authoritative for
  pi/omp/mastracode/opencode/kilo/kimi; session-only for hermes/qwen/letta/agy).
  OSC 0/2 titles and OSC 9;4 progress feed the rules. Process exit ⇒ `idle`.
  `done` is derived: `idle && !seen`; `seen` is set on any non-idle state and
  cleared on a working/blocked→idle completion transition (unless the operator
  is watching that tab).
- **Windows**: first-class. ConPTY through `portable-pty`; job objects
  (`KILL_ON_JOB_CLOSE`) clean up descendants; named pipe endpoint with a
  restrictive DACL; Ctrl-C is injected as byte 0x03 into the pty, never
  `GenerateConsoleCtrlEvent`.
- **ConPTY handshake (found while building the prototype)**: with
  `INHERIT_CURSOR`, ConPTY writes `ESC[6n` and blocks until the terminal answers
  with a cursor report. herdr answers from ghostty-vt (`on_read` returns
  `terminal_responses`); the prototype answers from `vt100`.

## Licence

Apache-2.0 (`Cargo.toml`); `LICENSE` appendix unfilled; **no** `NOTICE`; no
source headers. Vendored third-party: `portable-pty` MIT (Wez Furlong, 2018),
`libghostty-vt` MIT. Obligations when copying: ship the licence copy, mark
modified files (§4b), retain notices (§4c). This worktree ships
`LICENSE-APACHE`, a `NOTICE` with file-level provenance, and header comments in
every adapted file.

## Crate proposal (implemented in part B)

| Crate | Contains | herdr source |
|---|---|---|
| `hn-core-pty` | pty session (spawn/stream/resize/kill) over `portable-pty` | adapted from `src/pty`; actor simplified to reader/exit threads |
| `hn-core-detect` | agent identification, TOML rule engine, OSC title tracker, manifests | adapted from `src/detect` (alias subset, gate/region semantics, 3 agent manifests + 1 own) |
| `hn-core-auth` | trust store (devices + hashed codes), request verification (P-256/Ed25519, nonce cache, skew), authorization matrix, rate limits, tailnet owner check | new; behavioural reference is the Python gateway (`gateway/src/herdr_notify/api/auth/*`) and contracts `pairing.md`/`auth.md` |
| `hn-core-state` | reduced state, ranking, counts, `done` derivation, global `state_change_seq`, SSE frames, persisted counters | semantics from `app/actions.rs`, `app/api_helpers.rs` and contract `state.md` |
| `hn-core-api` | HTTP + SSE server: contract subset, signed-request middleware, pairing, device management | new; `EventHub` idea from `src/api/event_hub.rs` |
| `hn-core-host` | orchestration: pty → vt100 → detect → store; DSR answering; trailing-edge detection; `host` CLI | rewritten from `src/pane.rs`, without ratatui |

Dev-only (never shipped): `hn-core-test-support`, a minimal signed client for
tests and the demo, adapted from `desktop/core` and pinned to the gateway's
golden vectors.

Discarded for this core: ghostty-vt (Zig build), ratatui UI, kitty graphics,
sounds, remote, plugins, session persistence (next phase) and hooks
(`pane.report_agent`).
