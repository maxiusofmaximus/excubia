//! The operator-facing words of the typed refusals, one table per family of codes.

use serde_json::Value;

/// The rule behind a delegation refusal, in the language of the operator. A
/// `None` means the code is not a delegation rule at all.
pub fn delegation_text(code: &str) -> Option<&'static str> {
    Some(match code {
        "delegation_cancelled" => "La delegación ya había terminado o estaba cancelada.",
        "depth_exceeded" => {
            "La delegación supera la profundidad permitida (Alfred → lead → especialista)."
        }
        "pane_limit" => "El host ya tiene seis paneles vivos; no se puede abrir otro.",
        "cycle_detected" => {
            "La delegación repite un encargo reciente; se detuvo para evitar un ciclo."
        }
        "turn_budget" => "Este turno ya hizo doce delegaciones; no se permiten más.",
        "hour_budget" => "Este asiento ya hizo sesenta delegaciones en la última hora.",
        "seat_limit" => "El proyecto ya tiene ocho asientos; no cabe otro.",
        "scope_denied" => "Ese asiento no puede hacer esta delegación.",
        "lead_exists" => "El proyecto ya tiene un lead.",
        "project_exists" => "Ya existe un proyecto con ese nombre.",
        "not_your_delegation" => "Esa delegación pertenece a otro asiento.",
        "unknown_project" => "Ese proyecto no existe en el host.",
        "unknown_agent" => "Ese asiento no existe en el host.",
        "no_such_delegation" => "Esa delegación ya no existe en el host.",
        "delegation_timeout" => "El turno de la delegación agotó el tiempo de espera.",
        "pane_gone" => "El panel del asignado desapareció antes de terminar.",
        "host_restart" => "El host se reinició y la delegación quedó interrumpida.",
        "conversation_cancelled" => "Se canceló el equipo de esta conversación.",
        "user_cancelled" => "La delegación se canceló desde el escritorio.",
        _ => return None,
    })
}

/// What an approval conflict means to the operator. A `None` means the code is
/// not an approval rule at all.
pub fn approval_text(code: &str) -> Option<&'static str> {
    Some(match code {
        "approval_already_decided" => {
            "Otro dispositivo ya decidió esta aprobación. La tarjeta se ha actualizado."
        }
        "approval_expired" => "Se agotó el tiempo de esta aprobación: no se ejecutó.",
        "approval_digest_reused" => {
            "Esa acción ya se pidió antes: el host no abre otra aprobación."
        }
        "approval_mismatch" => "La acción cambió después de pedir permiso: no se ejecutó.",
        "approval_not_approved" => "Esta aprobación no está permitida: no se puede usar.",
        "approval_limit" => "La conversación ya tiene cuatro aprobaciones esperando.",
        _ => return None,
    })
}

/// What a team refusal means to the operator. A `None` means the code is not a
/// team rule at all.
pub fn team_text(code: &str, field: Option<&str>) -> Option<&'static str> {
    Some(match (code, field) {
        ("unknown_role", _) => "Ese profesional no está en el catálogo.",
        ("unknown_area", _) => "Esa área no existe en el catálogo.",
        ("invalid_field", Some("avatar")) => {
            "Ese personaje no existe. Elige uno de la lista o ninguno."
        }
        ("invalid_field", Some("roles")) => "Ese profesional no pertenece a las áreas elegidas.",
        ("invalid_field", Some("areas")) => "Elige al menos un área.",
        ("invalid_name", _) => {
            "El nombre debe tener entre 1 y 40 caracteres, sin caracteres de control."
        }
        ("area_full", _) => "Esa área ya tiene 8 agentes. Quita alguno y vuelve a intentarlo.",
        ("hire_not_active", _) => "Ese agente ya no está en tu equipo: lo despediste.",
        ("no_such_hire", _) => "Ese agente ya no existe en el equipo.",
        _ => return None,
    })
}

/// The words of a refused alert rule. A `None` means the code is not a rule refusal.
pub(super) fn alert_text(code: &str) -> Option<&'static str> {
    Some(match code {
        "rule_exists" => "Ya existe una regla igual",
        "rule_limit" => "Has llegado al máximo de 100 reglas",
        "not_found" => "Esa regla ya no existe.",
        _ => return None,
    })
}

/// The team's refusals carry a stable code and field in `detail`. Returns the
/// code and its text, or `None` when the body is not a team rule.
pub(super) fn team_refusal(body: &[u8]) -> Option<(String, String)> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let detail = value.get("detail")?.as_object()?;
    let code = detail.get("code")?.as_str()?;
    let field = detail.get("field").and_then(Value::as_str);
    team_text(code, field).map(|message| (code.to_string(), message.to_string()))
}
