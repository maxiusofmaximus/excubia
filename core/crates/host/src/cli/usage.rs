//! The usage text and the canonical grant command.

/// The executable name every user-facing command line starts with.
pub const BIN: &str = "excubia-host";

pub const USAGE: &str = "excubia-host [--port N] [--bind ADDR] [--rows R] [--cols C] [--label L] \
[--tls-bind IP:PORT]... [--hook-ttl SECONDS] [--snapshot-lines N] \
[--workspaces-root DIR] [--uac-plugin-dir DIR] [--tools-dir DIR] \
[--asr-python PY --asr-script PY_FILE --asr-model DIR] [--asr-no-corrector] [--mail-sidecar http://127.0.0.1:PORT] [--spawn \"<cmdline>\"]... [--data-dir DIR]\n\
       excubia-host pair-code [--ttl SECONDS, default 180] [--lan IP:PORT]... [--data-dir DIR]\n\
       excubia-host pair-code --pake [--uri] [--port N, default 8791] [--data-dir DIR]\n\
       excubia-host tls prepare|activate [--data-dir DIR]\n\
       excubia-host hook-token [--seat AGENT_ID] [--data-dir DIR]\n\
       excubia-host claude-settings --hook <excubia-uac-hook binary> [--out FILE]\n\
       excubia-host strata start | stop | status | enable-agent [--data-dir DIR]\n\
       excubia-host alfred local | use local|claude | status [--data-dir DIR]\n\
       excubia-host audit tail [--n N, default 50] [--data-dir DIR]\n\
       excubia-host devices list [--data-dir DIR]\n\
       excubia-host devices revoke <device_id> [--data-dir DIR]\n\
       excubia-host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]\n\
       excubia-host devices recovery <device_id> [--data-dir DIR]";

/// The canonical grant command, exactly as the host CLI accepts it. The
/// contracts and the clients' hints quote this text; a test keeps it equal to
/// the CLI usage and to the permission labels.
pub const GRANT_HINT: &str = "excubia-host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]";
