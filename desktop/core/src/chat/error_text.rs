//! The operator-facing words of the typed refusals, one table per family of codes.

use serde_json::Value;

use crate::ui_lang::{self, UiLang};

/// The rule behind a delegation refusal, in the language of the operator. A
/// `None` means the code is not a delegation rule at all.
pub fn delegation_text(code: &str) -> Option<&'static str> {
    delegation_text_in(code, ui_lang::current())
}

pub fn delegation_text_in(code: &str, lang: UiLang) -> Option<&'static str> {
    Some(match code {
        "delegation_cancelled" => lang.pick(
            "La delegación ya había terminado o estaba cancelada.",
            "The delegation had already finished or was cancelled.",
        ),
        "depth_exceeded" => lang.pick(
            "La delegación supera la profundidad permitida (Alfred → lead → especialista).",
            "The delegation exceeds the allowed depth (Alfred → lead → specialist).",
        ),
        "pane_limit" => lang.pick(
            "El host ya tiene seis paneles vivos; no se puede abrir otro.",
            "The host already has six live panes; another one cannot be opened.",
        ),
        "cycle_detected" => lang.pick(
            "La delegación repite un encargo reciente; se detuvo para evitar un ciclo.",
            "The delegation repeats a recent assignment; it stopped to avoid a loop.",
        ),
        "turn_budget" => lang.pick(
            "Este turno ya hizo doce delegaciones; no se permiten más.",
            "This turn already made twelve delegations; no more are allowed.",
        ),
        "hour_budget" => lang.pick(
            "Este asiento ya hizo sesenta delegaciones en la última hora.",
            "This seat already made sixty delegations in the last hour.",
        ),
        "seat_limit" => lang.pick(
            "El proyecto ya tiene ocho asientos; no cabe otro.",
            "The project already has eight seats; there is no room for another.",
        ),
        "scope_denied" => lang.pick(
            "Ese asiento no puede hacer esta delegación.",
            "That seat cannot make this delegation.",
        ),
        "lead_exists" => lang.pick("El proyecto ya tiene un lead.", "The project already has a lead."),
        "project_exists" => lang.pick(
            "Ya existe un proyecto con ese nombre.",
            "A project with that name already exists.",
        ),
        "not_your_delegation" => lang.pick(
            "Esa delegación pertenece a otro asiento.",
            "That delegation belongs to another seat.",
        ),
        "unknown_project" => lang.pick(
            "Ese proyecto no existe en el host.",
            "That project does not exist on the host.",
        ),
        "unknown_agent" => lang.pick(
            "Ese asiento no existe en el host.",
            "That seat does not exist on the host.",
        ),
        "no_such_delegation" => lang.pick(
            "Esa delegación ya no existe en el host.",
            "That delegation no longer exists on the host.",
        ),
        "delegation_timeout" => lang.pick(
            "El turno de la delegación agotó el tiempo de espera.",
            "The delegation's turn ran out of time.",
        ),
        "wait_unavailable" => lang.pick(
            "El host no pudo leer los permisos pendientes de la delegación.",
            "The host could not read the delegation's pending approvals.",
        ),
        "pane_gone" => lang.pick(
            "El panel del asignado desapareció antes de terminar.",
            "The assignee's pane disappeared before it finished.",
        ),
        "host_restart" => lang.pick(
            "El host se reinició y la delegación quedó interrumpida.",
            "The host restarted and the delegation was interrupted.",
        ),
        "conversation_cancelled" => lang.pick(
            "Se canceló el equipo de esta conversación.",
            "The team for this conversation was cancelled.",
        ),
        "user_cancelled" => lang.pick(
            "La delegación se canceló desde el escritorio.",
            "The delegation was cancelled from the desktop.",
        ),
        _ => return None,
    })
}

/// What an approval conflict means to the operator. A `None` means the code is
/// not an approval rule at all.
pub fn approval_text(code: &str) -> Option<&'static str> {
    approval_text_in(code, ui_lang::current())
}

pub fn approval_text_in(code: &str, lang: UiLang) -> Option<&'static str> {
    Some(match code {
        "approval_already_decided" => lang.pick(
            "Otro dispositivo ya decidió esta aprobación. La tarjeta se ha actualizado.",
            "Another device already decided this approval. The card has been updated.",
        ),
        "approval_expired" => lang.pick(
            "Se agotó el tiempo de esta aprobación: no se ejecutó.",
            "This approval timed out: nothing was run.",
        ),
        "approval_digest_reused" => lang.pick(
            "Esa acción ya se pidió antes: el host no abre otra aprobación.",
            "That action was already requested: the host will not open another approval.",
        ),
        "approval_mismatch" => lang.pick(
            "La acción cambió después de pedir permiso: no se ejecutó.",
            "The action changed after permission was requested: nothing was run.",
        ),
        "approval_not_approved" => lang.pick(
            "Esta aprobación no está permitida: no se puede usar.",
            "This approval is not allowed: it cannot be used.",
        ),
        "approval_limit" => lang.pick(
            "La conversación ya tiene cuatro aprobaciones esperando.",
            "The conversation already has four approvals waiting.",
        ),
        _ => return None,
    })
}

/// What a team refusal means to the operator. A `None` means the code is not a
/// team rule at all.
pub fn team_text(code: &str, field: Option<&str>) -> Option<&'static str> {
    team_text_in(code, field, ui_lang::current())
}

pub fn team_text_in(code: &str, field: Option<&str>, lang: UiLang) -> Option<&'static str> {
    Some(match (code, field) {
        ("unknown_role", _) => lang.pick(
            "Ese profesional no está en el catálogo.",
            "That professional is not in the catalog.",
        ),
        ("unknown_area", _) => lang.pick(
            "Esa área no existe en el catálogo.",
            "That area does not exist in the catalog.",
        ),
        ("invalid_field", Some("avatar")) => lang.pick(
            "Ese personaje no existe. Elige uno de la lista o ninguno.",
            "That character does not exist. Choose one from the list or none.",
        ),
        ("invalid_field", Some("roles")) => lang.pick(
            "Ese profesional no pertenece a las áreas elegidas.",
            "That professional does not belong to the chosen areas.",
        ),
        ("invalid_field", Some("areas")) => {
            lang.pick("Elige al menos un área.", "Choose at least one area.")
        }
        ("invalid_name", _) => lang.pick(
            "El nombre debe tener entre 1 y 40 caracteres, sin caracteres de control.",
            "The name must have between 1 and 40 characters, with no control characters.",
        ),
        ("area_full", _) => lang.pick(
            "Esa área ya tiene 8 agentes. Quita alguno y vuelve a intentarlo.",
            "That area already has 8 agents. Remove one and try again.",
        ),
        ("hire_not_active", _) => lang.pick(
            "Ese agente ya no está en tu equipo: lo despediste.",
            "That agent is no longer on your team: you dismissed it.",
        ),
        ("no_such_hire", _) => lang.pick(
            "Ese agente ya no existe en el equipo.",
            "That agent no longer exists on the team.",
        ),
        ("no_resident_engine", _) => lang.pick(
            "Ningún CLI instalado puede abrir un panel residente. Instala Claude Code, Codex u OpenCode en una versión admitida y vuelve a intentarlo.",
            "No installed CLI can open a resident panel. Install Claude Code, Codex or OpenCode at a supported version and try again.",
        ),
        ("resident_unsupported", _) => lang.pick(
            "Ese CLI no tiene modo de panel residente en Excubia. Actualiza Excubia o instala Claude Code, Codex u OpenCode y vuelve a intentarlo.",
            "That CLI has no resident panel mode in Excubia. Update Excubia or install Claude Code, Codex or OpenCode and try again.",
        ),
        ("uac_unsafe", _) => lang.pick(
            "El control de aprobaciones del CLI no pasó su comprobación, así que el panel no se abre. Actualiza el CLI o su plugin y vuelve a intentarlo.",
            "The CLI's approval check did not pass, so the panel does not open. Update the CLI or its plugin and try again.",
        ),
        _ => return None,
    })
}

/// What a failed turn's reason means to the operator. The reason is `code; last output`, and
/// only the code decides; a `None` means the code is not an engine rule.
pub fn run_failure_text(reason: &str) -> Option<&'static str> {
    run_failure_text_in(reason, ui_lang::current())
}

pub fn run_failure_text_in(reason: &str, lang: UiLang) -> Option<&'static str> {
    let code = reason.split_once(';').map_or(reason, |(code, _)| code).trim();
    Some(match code {
        "free_models_exhausted" => lang.pick(
            "Los modelos gratuitos de OpenCode han llegado a su límite de uso. Vuelve a intentarlo más tarde o conecta otro motor en Ajustes.",
            "The free OpenCode models have reached their usage limit. Try again later or connect another engine in Settings.",
        ),
        _ => return None,
    })
}

/// The words of a refused alert rule. A `None` means the code is not a rule refusal.
pub(super) fn alert_text(code: &str) -> Option<&'static str> {
    alert_text_in(code, ui_lang::current())
}

pub(super) fn alert_text_in(code: &str, lang: UiLang) -> Option<&'static str> {
    Some(match code {
        "rule_exists" => lang.pick("Ya existe una regla igual", "An identical rule already exists"),
        "rule_limit" => lang.pick(
            "Has llegado al máximo de 100 reglas",
            "You have reached the limit of 100 rules",
        ),
        "not_found" => lang.pick("Esa regla ya no existe.", "That rule no longer exists."),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spanish_rule_words_are_the_default() {
        assert!(delegation_text_in("pane_limit", UiLang::Es)
            .unwrap()
            .contains("seis paneles"));
    }

    #[test]
    fn an_english_delegation_refusal_keeps_the_chain_of_roles() {
        let text = delegation_text_in("depth_exceeded", UiLang::En).unwrap();
        assert_eq!(
            text,
            "The delegation exceeds the allowed depth (Alfred → lead → specialist)."
        );
    }

    #[test]
    fn an_invalid_field_is_worded_by_its_own_field_in_english() {
        let roles = team_text_in("invalid_field", Some("roles"), UiLang::En).unwrap();
        let areas = team_text_in("invalid_field", Some("areas"), UiLang::En).unwrap();
        assert_eq!(roles, "That professional does not belong to the chosen areas.");
        assert_eq!(areas, "Choose at least one area.");
    }

    #[test]
    fn each_resident_refusal_says_what_to_do_in_both_languages() {
        // The three refusals are told apart: no engine, no resident mode, unsafe approvals.
        for code in ["no_resident_engine", "resident_unsupported", "uac_unsafe"] {
            let es = team_text_in(code, None, UiLang::Es).expect("spanish remedy");
            let en = team_text_in(code, None, UiLang::En).expect("english remedy");
            assert_ne!(es, en, "{code}");
            assert!(es.contains("Actualiza") || es.contains("Instala"), "{code}: {es}");
            assert!(en.contains("Update") || en.contains("Install"), "{code}: {en}");
        }
        assert!(team_text_in("no_resident_engine", None, UiLang::Es)
            .unwrap()
            .contains("Ningún CLI"));
        assert!(team_text_in("resident_unsupported", None, UiLang::En)
            .unwrap()
            .contains("no resident panel mode"));
    }

    #[test]
    fn an_exhausted_free_model_says_what_to_do_in_both_languages() {
        // The stored reason carries the CLI's last output after the code; only the code decides.
        let reason = "free_models_exhausted; última salida: Free usage limit reached";
        let es = run_failure_text_in(reason, UiLang::Es).expect("spanish");
        let en = run_failure_text_in(reason, UiLang::En).expect("english");
        assert!(es.contains("límite de uso") && es.contains("Ajustes"), "{es}");
        assert!(en.contains("usage limit") && en.contains("Settings"), "{en}");
        assert_ne!(es, en);
        assert_eq!(run_failure_text_in("process_exit: 1", UiLang::En), None);
    }

    #[test]
    fn a_code_outside_the_table_is_not_a_rule_in_either_language() {
        assert_eq!(delegation_text_in("unknown_code", UiLang::En), None);
        assert_eq!(approval_text_in("unknown_code", UiLang::Es), None);
        assert_eq!(alert_text_in("unknown_code", UiLang::En), None);
    }
}
