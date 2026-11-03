//! The usage text and the canonical grant command.

pub const USAGE: &str = "host [--port N] [--bind ADDR] [--rows R] [--cols C] [--label L] \
[--tls-bind IP:PORT]... [--hook-ttl SECONDS] [--snapshot-lines N] \
[--workspaces-root DIR] [--uac-plugin-dir DIR] \
[--asr-python PY --asr-script PY_FILE --asr-model DIR] [--asr-no-corrector] [--mail-sidecar http://127.0.0.1:PORT] [--spawn \"<cmdline>\"]... [--data-dir DIR]\n\
       host pair-code [--ttl SECONDS, default 180] [--lan IP:PORT]... [--data-dir DIR]\n\
       host pair-code --pake [--port N, default 8791] [--data-dir DIR]\n\
       host tls prepare|activate [--data-dir DIR]\n\
       host hook-token [--seat AGENT_ID] [--data-dir DIR]\n\
       host claude-settings --hook <hn-uac-hook binary> [--out FILE]\n\
       host strata start | stop | status | enable-agent [--data-dir DIR]\n\
       host alfred local | use local|claude | status [--data-dir DIR]\n\
       host devices list [--data-dir DIR]\n\
       host devices revoke <device_id> [--data-dir DIR]\n\
       host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]\n\
       host devices recovery <device_id> [--data-dir DIR]";

/// The canonical grant command, exactly as the host CLI accepts it. The
/// contracts and the clients' hints quote this text; a test keeps it equal to
/// the CLI usage and to the permission labels.
pub const GRANT_HINT: &str = "host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]";
