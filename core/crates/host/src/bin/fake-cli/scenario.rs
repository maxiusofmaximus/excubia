/// The task text without the test markers: a real seat never sees them.
pub(super) fn strip_markers(text: &str) -> String {
    let mut out = text.to_string();
    for marker in [
        "[[fake:lead-spawn]]",
        "[[fake:complete]]",
        "[[fake:silent]]",
        "[[fake:hang]]",
    ] {
        out = out.replace(marker, "");
    }
    out.trim().to_string()
}

pub(super) fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    if line.chars().count() > 120 {
        let mut bounded: String = line.chars().take(119).collect();
        bounded.push('…');
        return bounded;
    }
    line.to_string()
}

/// The text between `key` and the next quote: how a resumed plan names its plan and seat.
pub(super) fn quoted(text: &str, key: &str) -> Option<String> {
    let rest = &text[text.find(key)? + key.len()..];
    Some(rest[..rest.find('"')?].to_string())
}

pub(super) fn scenario(prompt: &str) -> &'static str {
    // The resumed turn is checked first: it may quote the history of the chat.
    if prompt.contains("aprobó el plan") {
        return "resume";
    }
    for (marker, name) in [
        // The team marker comes first: a team message may also carry the
        // hang or silent marker of the task it delegates.
        ("[[fake:team]]", "team"),
        ("[[fake:plan]]", "plan"),
        ("[[fake:uac-mcp]]", "uac-mcp"),
        ("[[fake:silent]]", "silent"),
        ("[[fake:hang]]", "hang"),
        ("[[fake:invalid]]", "invalid"),
        ("[[fake:inject]]", "inject"),
        ("[[fake:ask]]", "ask"),
        ("[[fake:env]]", "env"),
        ("[[fake:quota]]", "quota"),
        ("[[fake:two-replies]]", "two-replies"),
    ] {
        if prompt.contains(marker) {
            return name;
        }
    }
    "reply"
}
