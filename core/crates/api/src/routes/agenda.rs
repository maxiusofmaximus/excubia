//! The agenda's routes (AGENDA-REST): notes and contacts, merged into the protected
//! router. Each path's permission is in `hn-core-auth`'s `authz/agenda.rs`.

use axum::routing::{get, post};
use axum::Router;

use super::{alerts, calendar, calendar_ics, contacts, notes};
use crate::ApiContext;

pub fn agenda_routes() -> Router<ApiContext> {
    Router::new()
        .route(
            "/v1/calendar/events",
            get(calendar::list).post(calendar::create),
        )
        .route(calendar_ics::IMPORT_PATH, post(calendar_ics::import))
        .route("/v1/calendar/export.ics", get(calendar_ics::export))
        .route("/v1/calendar/events/{event_id}", get(calendar::show))
        .route(
            "/v1/calendar/events/{event_id}/update",
            post(calendar::update),
        )
        .route(
            "/v1/calendar/events/{event_id}/delete",
            post(calendar::delete),
        )
        .route(
            "/v1/company/timezone",
            get(calendar::timezone).post(calendar::set_timezone),
        )
        .route("/v1/notes", get(notes::list).post(notes::create))
        .route("/v1/notes/{note_id}", get(notes::show))
        .route("/v1/notes/{note_id}/update", post(notes::update))
        .route("/v1/notes/{note_id}/delete", post(notes::delete))
        .route("/v1/contacts", get(contacts::list).post(contacts::create))
        .route("/v1/contacts/import", post(contacts::import))
        .route("/v1/contacts/{contact_id}", get(contacts::show))
        .route("/v1/contacts/{contact_id}/update", post(contacts::update))
        .route("/v1/contacts/{contact_id}/delete", post(contacts::delete))
        .route("/v1/alerts/rules", get(alerts::list).post(alerts::create))
        .route("/v1/alerts/rules/{rule_id}/update", post(alerts::update))
        .route("/v1/alerts/rules/{rule_id}/delete", post(alerts::delete))
        .route(
            "/v1/alerts/mode",
            get(alerts::show_mode).post(alerts::set_mode),
        )
}
