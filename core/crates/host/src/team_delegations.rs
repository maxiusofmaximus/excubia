//! The delegation tree at runtime: the assignment, its delivery to the
//! resident panel, its watcher, the card state and the coalesced synthesis.

use std::sync::Arc;

use excubia_chat::{
    close_title, preview, without_internal_ids, ChatError, ChatSeat, Delegation, DelegationState,
    Launch, NewDelegation, RecipeRun, SeatActor, SynthesisRow, ToolError, ToolReply,
};
use serde_json::json;

use super::team::{lock, TeamGrant, TeamService, Watcher, SYNTHESIS_BATCH};
use super::team_errors::tool_chat_error;
use super::team_status::delegation_sentence;

impl TeamService {
    pub(super) async fn assign(
        &self,
        actor: &SeatActor,
        assign: excubia_seat::DelegationAssign,
    ) -> Result<ToolReply, ToolError> {
        let parent = if actor.role_id == "lead" {
            Some(
                self.chat
                    .active_delegation_for_agent(&actor.agent_id)
                    .map_err(tool_chat_error)?
                    .ok_or_else(|| {
                        ToolError::invalid(
                            "scope_denied",
                            "tool",
                            "the lead has no active delegation",
                        )
                    })?,
            )
        } else {
            None
        };
        let conversation_id = match &parent {
            Some(parent) => parent.conversation_id.clone(),
            None => assign
                .conversation_id
                .clone()
                .unwrap_or_else(|| actor.conversation_id.clone()),
        };
        if conversation_id.is_empty() {
            return Err(ToolError::invalid(
                "invalid_arguments",
                "conversation_id",
                "conversation_id is required",
            ));
        }
        let to_agent_id = self
            .seat_of_target(&assign.to_agent_id, &conversation_id)
            .await?;
        // Alfred runs several projects: assigning a lead puts the conversation on that
        // lead's project, unless work the conversation delegated elsewhere is still open.
        if actor.role_id == "alfred" {
            let seat = self.chat.seat(&to_agent_id).map_err(tool_chat_error)?;
            // A project the operator opened from a folder has a lead that never ran:
            // it starts the first time work goes to it.
            if seat.role_id == "lead" && seat.pane_id.is_none() {
                self.spawn_panel(&seat).await?;
            }
            if let Some(project_id) = seat.project_id.as_deref() {
                self.chat
                    .set_conversation_project(&conversation_id, project_id)
                    .map_err(|_| {
                        ToolError::invalid(
                            "project_busy",
                            "to_agent_id",
                            "this conversation still has open work in another project; wait for it or use a new conversation",
                        )
                    })?;
            }
        }
        let delegation = self
            .chat
            .create_delegation(NewDelegation {
                conversation_id,
                from_agent_id: actor.agent_id.clone(),
                to_agent_id,
                task: assign.task,
                turn_key: parent
                    .as_ref()
                    .map(|parent| parent.id.clone())
                    .unwrap_or_else(|| actor.run_id.clone()),
                parent_id: parent.as_ref().map(|parent| parent.id.clone()),
                steps: assign.steps,
                plan_id: assign.plan_id,
            })
            .map_err(tool_chat_error)?;
        self.dispatch(&delegation).await?;
        Ok(ToolReply {
            result: json!({
                "delegation_id": delegation.id,
                // The pane accepted the task when dispatch returned: one reading for the model.
                "status": "accepted",
                "summary": format!("{} recibió el encargo.", delegation.to_name),
                "pane_id": delegation.pane_id,
                "depth": delegation.depth,
            }),
        })
    }

    /// A recipe run: the hire's panel is spawned if it has none, the delegation
    /// starts from Alfred and the body is delivered. The use counts once it is out.
    pub(super) async fn start_recipe(
        &self,
        run: RecipeRun,
        steps: u32,
        plan_id: Option<String>,
    ) -> Result<Delegation, ToolError> {
        let to_agent_id = self
            .seat_of_target(&run.hire_id, &run.conversation_id)
            .await?;
        let delegation = self
            .chat
            .create_user_delegation(NewDelegation {
                conversation_id: run.conversation_id,
                from_agent_id: run.alfred_id,
                to_agent_id,
                task: run.task,
                turn_key: run.turn_key,
                parent_id: None,
                steps,
                plan_id,
            })
            .map_err(tool_chat_error)?;
        self.dispatch(&delegation).await?;
        if let Some(recipe_id) = &run.recipe_id {
            self.chat
                .count_recipe_use(recipe_id)
                .map_err(tool_chat_error)?;
        }
        Ok(delegation)
    }

    /// A scheduled run: the same path as a recipe run, but its plan was checked when the
    /// schedule was saved, so the gate is passed here and no recipe use is counted.
    pub(super) async fn start_scheduled(&self, launch: Launch) -> Result<Delegation, ToolError> {
        let run = launch.run;
        let to_agent_id = self
            .seat_of_target(&run.hire_id, &run.conversation_id)
            .await?;
        let delegation = self
            .chat
            .create_schedule_delegation(NewDelegation {
                conversation_id: run.conversation_id,
                from_agent_id: run.alfred_id,
                to_agent_id,
                task: run.task,
                turn_key: run.turn_key,
                parent_id: None,
                steps: launch.steps,
                plan_id: None,
            })
            .map_err(tool_chat_error)?;
        self.dispatch(&delegation).await?;
        Ok(delegation)
    }

    /// Start the delegation and deliver it to the resident panel. A missing
    /// panel fails the delegation now, never leaves it queued forever.
    pub(super) async fn dispatch(&self, delegation: &Delegation) -> Result<(), ToolError> {
        let Some(pane_id) = delegation.pane_id.clone() else {
            // No panel: the delegation fails now, it is never left queued.
            let _ = self
                .chat
                .fail_delegation(&delegation.id, "seat_unavailable");
            return Err(ToolError::conflict(
                "seat_unavailable",
                "the assignee has no live panel",
            ));
        };
        let _ = self
            .chat
            .start_delegation(&delegation.id)
            .map_err(tool_chat_error)?;
        let task = json!({
            "delegation_id": delegation.id,
            "from": delegation.from_name,
            "task": delegation.task,
        })
        .to_string();
        if let Err(error) = self.host.write_line(&pane_id, &task) {
            let _ = self.chat.fail_delegation(&delegation.id, "pane_gone");
            return Err(ToolError::conflict(
                "seat_unavailable",
                format!("could not deliver the task: {error}"),
            ));
        }
        self.watch(delegation.id.clone(), pane_id);
        Ok(())
    }

    pub(super) fn watch(&self, delegation_id: String, _pane_id: String) {
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let chat = Arc::clone(&self.chat);
        let timeout = self.delegation_timeout;
        let id = delegation_id.clone();
        let task = tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(timeout) => {
                    let _ = chat.fail_delegation(&id, "delegation_timeout");
                }
                _ = stopped => {}
            }
        });
        if let Some(previous) = lock(&self.watchers).insert(delegation_id, Watcher { stop, task }) {
            let _ = previous.stop.send(());
            previous.task.abort();
        }
    }

    pub(super) fn stop_watcher(&self, delegation_id: &str) {
        if let Some(watcher) = lock(&self.watchers).remove(delegation_id) {
            let _ = watcher.stop.send(());
            watcher.task.abort();
        }
    }

    pub(super) fn complete(
        &self,
        grant: TeamGrant,
        delegation_id: String,
        result: String,
    ) -> Result<ToolReply, ToolError> {
        let delegation = self
            .chat
            .delegation(&delegation_id)
            .map_err(tool_chat_error)?;
        if delegation.to_agent_id != grant.actor.agent_id {
            return Err(ToolError::conflict(
                "not_your_delegation",
                "this delegation belongs to another seat",
            ));
        }
        let message_id = self
            .chat
            .complete_delegation(&delegation_id, &result)
            .map_err(tool_chat_error)?;
        self.stop_watcher(&delegation_id);
        let _ = self.chat.set_seat_status(&grant.actor.agent_id, "idle");
        self.synthesis.notify_one();
        Ok(ToolReply {
            result: json!({
                "delegation_id": delegation_id,
                "status": "done",
                "summary": delegation_sentence(&delegation.to_name, DelegationState::Done),
                "message_id": message_id,
            }),
        })
    }

    /// One Alfred turn per pending batch. If Alfred is busy or the queue is
    /// full the cards stay authoritative and the batch is skipped, never
    /// retried in a loop.
    pub(super) async fn synthesis_loop(&self) {
        loop {
            self.synthesis.notified().await;
            loop {
                let rows = match self.chat.pending_synthesis(SYNTHESIS_BATCH) {
                    Ok(rows) => rows,
                    Err(error) => {
                        eprintln!("team: could not read pending synthesis: {error}");
                        break;
                    }
                };
                let Some(first) = rows.first() else { break };
                let conversation_id = first.conversation_id.clone();
                let batch: Vec<_> = rows
                    .into_iter()
                    .filter(|row| row.conversation_id == conversation_id)
                    .collect();
                let alfred_id = match self.chat.alfred_id() {
                    Ok(agent_id) => agent_id,
                    Err(error) => {
                        eprintln!("team: no Alfred seat: {error}");
                        break;
                    }
                };
                // One finished delegation is closed by the host, with no model turn, unless
                // it was a question: then Alfred answers it in the chat.
                let answers = single_row(&batch).is_some_and(|row| self.wants_answer(row));
                if let Some(row) = single_row(&batch).filter(|_| !answers) {
                    match self.close_by_host(row, &alfred_id) {
                        Ok(()) => continue,
                        Err(error) => {
                            eprintln!("team: could not write the close: {error}");
                            let _ = self.chat.skip_synthesis(&batch, "close_unavailable");
                            break;
                        }
                    }
                }
                let text = match self.synthesis_text(&batch) {
                    Ok(text) => text,
                    Err(error) => {
                        eprintln!("team: could not build synthesis: {error}");
                        let _ = self.chat.skip_synthesis(&batch, "synthesis_unavailable");
                        break;
                    }
                };
                let turn = match self.chat.enqueue_synthesis(&batch, &alfred_id, &text) {
                    Ok(turn) => turn,
                    Err(error) => {
                        eprintln!("team: could not enqueue synthesis: {error}");
                        let _ = self.chat.skip_synthesis(&batch, "synthesis_unavailable");
                        break;
                    }
                };
                match self.alfred.try_enqueue(turn) {
                    Ok(()) => {}
                    Err(error) => {
                        let _ = self.chat.skip_synthesis(&batch, &format!("alfred_{error}"));
                    }
                }
            }
        }
    }

    pub(super) fn synthesis_text(
        &self,
        rows: &[excubia_chat::SynthesisRow],
    ) -> Result<String, ChatError> {
        let mut lines = Vec::with_capacity(rows.len());
        let mut denied = false;
        for row in rows {
            let detail = self.chat.delegation_detail(&row.delegation_id)?;
            let body = detail
                .result_message
                .as_ref()
                .map_or("", |message| message.body_md.as_str());
            denied |= body.contains("denegó") || body.contains("denegada");
            lines.push(format!(
                "- {} → {}: {} => {}",
                detail.delegation.from_name,
                detail.delegation.to_name,
                preview(&detail.delegation.task),
                excerpt(body)
            ));
        }
        Ok(synthesis_prompt(&lines, denied))
    }

    /// A finished consult (a question, not an order) whose answer the chat must show: the
    /// host's fixed line would only repeat the question. A denied or failed task keeps its line.
    fn wants_answer(&self, row: &SynthesisRow) -> bool {
        let Ok(delegation) = self.chat.delegation(&row.delegation_id) else {
            return false;
        };
        delegation.state == DelegationState::Done
            && asks_for_answer(&delegation.task)
            && !self
                .chat
                .delegation_denied_by_operator(&row.delegation_id)
                .unwrap_or(true)
    }

    /// The close of one finished delegation, written by the host: a fixed sentence as
    /// Alfred's line, through the same turn path a reply takes, but no model runs.
    fn close_by_host(&self, row: &SynthesisRow, alfred_id: &str) -> Result<(), ChatError> {
        let delegation = self.chat.delegation(&row.delegation_id)?;
        let denied = self
            .chat
            .delegation_denied_by_operator(&row.delegation_id)?;
        let outcome = if denied {
            CloseOutcome::Denied
        } else if delegation.state == DelegationState::Failed {
            CloseOutcome::Failed
        } else {
            CloseOutcome::Done
        };
        let reason = if denied {
            self.chat.delegation_denial_reason(&row.delegation_id)?
        } else {
            None
        };
        let text = close_sentence(
            &delegation.to_name,
            &close_title(&delegation.task),
            outcome,
            reason.as_deref(),
        );
        let rows = std::slice::from_ref(row);
        let turn = self.chat.enqueue_synthesis(rows, alfred_id, &text)?;
        if let Err(error) = self.chat.complete_run_with_reply(&turn.id, &text, &[]) {
            let _ = self.chat.fail_run(&turn.id, "synthesis_unavailable");
            return Err(error);
        }
        self.chat.mark_synthesis_done(rows)
    }

    /// `pane.*` is the live source of the card state: blocked -> waiting,
    /// working -> running, closed -> failed and seat stopped.
    pub(super) async fn listen_to_panes(&self) {
        let mut events = self.host.subscribe();
        loop {
            let frame = match events.recv().await {
                Ok(frame) => frame,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            let Some(pane_id) = frame.body["pane_id"].as_str().map(str::to_string) else {
                continue;
            };
            if frame.event == excubia_state::EVENT_STATUS {
                let state = match frame.body["agent_status"].as_str() {
                    Some("blocked") => Some(DelegationState::Waiting),
                    Some("working") => Some(DelegationState::Running),
                    _ => None,
                };
                if let Some(state) = state {
                    if let Err(error) = self.chat.delegation_state_from_pane(&pane_id, state) {
                        eprintln!("team: pane state for {pane_id}: {error}");
                    }
                }
            } else if frame.event == excubia_state::EVENT_CLOSED {
                if let Ok(Some(delegation)) = self.chat.active_delegation_for_pane(&pane_id) {
                    self.stop_watcher(&delegation.id);
                }
                let _ = self.chat.fail_delegation_for_pane(&pane_id);
                let _ = self.chat.clear_seat_pane(&pane_id);
                self.remove_grant_for_pane(&pane_id);
            }
        }
    }
}

/// Whether a task asks to know something (a look or a question), so its result is the answer.
/// shortcut: a word list, upgrade with a flag the assignment carries.
fn asks_for_answer(task: &str) -> bool {
    let task = task.to_lowercase();
    [
        "solo lectura",
        "solo consulta",
        "sin cambiar",
        "no cambies",
        "indica ",
        "dime ",
        "diga ",
        "cuál",
        "cuáles",
        "qué ",
        "cuántos",
        "cuántas",
    ]
    .iter()
    .any(|word| task.contains(word))
}

/// The start of a report on one line, bounded: enough for Alfred to quote the answer.
fn excerpt(text: &str) -> String {
    const MAX_CHARS: usize = 700;
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    line.chars().take(MAX_CHARS).collect()
}

/// The turn Alfred gets when leads finish. Its rules come before the lines: the
/// order of a turn weighs more with the model than the seed's wording.
fn synthesis_prompt(lines: &[String], denied: bool) -> String {
    let mut text = String::from(
        "Responde con UNA frase de 25 palabras como máximo: qué hizo cada líder, con su nombre. \
         Si el encargo era una pregunta o una consulta, la frase da la respuesta (hasta 40 \
         palabras, sin tablas ni listas), no repite la pregunta y empieza por la respuesta, sin \
         introducción ni cierre: nada de «es su informe», «no lo he comprobado», «no tengo \
         acceso» ni hashes. Di solo lo que se preguntó: sin comandos («git add»), sin siglas («PR»), \
         sin ramas ni datos que no pidió, y «archivos nuevos» en vez de «sin seguimiento». \
         Solo el nombre del archivo, nunca la ruta. No uses herramientas. No repitas el informe, \
         no comentes lo que no comprobaste ni lo que no puedes hacer, no uses «cola», «delegación», \
         «reposo», «hook», «shell» ni «según su informe», ni identificadores internos, ni comandos \
         en bloque de código. Si algo no se hizo porque caducó una aprobación, dilo con esas \
         palabras sencillas. Sin listas.\n",
    );
    if denied {
        text.push_str(
            "El usuario denegó la acción. Di en una frase que no se hizo y cita su motivo si lo hay. \
             No preguntes si quieres insistir ni ofrezcas hacerlo de todos modos.\n",
        );
    }
    text.push_str("Lo que hizo cada líder:\n");
    text.push_str(&lines.join("\n"));
    text
}

/// The batch the host closes by itself: exactly one delegation. Larger batches go to Alfred.
fn single_row(batch: &[SynthesisRow]) -> Option<&SynthesisRow> {
    match batch {
        [row] => Some(row),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CloseOutcome {
    Done,
    Failed,
    Denied,
}

/// Alfred's fixed line for a closed delegation. It names the seat and the task, and the
/// operator's own words when he denied with some.
fn close_sentence(name: &str, title: &str, outcome: CloseOutcome, reason: Option<&str>) -> String {
    // A cut title already ends in «…»: no second stop after it.
    let stop = if title.ends_with('…') { "" } else { "." };
    match outcome {
        CloseOutcome::Done => format!("{name} terminó: {title}{stop}"),
        CloseOutcome::Failed => format!("{name} no pudo terminar: {title}{stop}"),
        CloseOutcome::Denied => match reason.and_then(close_reason) {
            Some(words) => format!("No se hizo: {title}{stop} Lo denegaste: «{words}»."),
            None => format!("No se hizo: {title}{stop} Lo denegaste."),
        },
    }
}

/// The operator's words for a sentence: one line, no internal ids, at most 80 characters.
fn close_reason(reason: &str) -> Option<String> {
    let one_line = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean = without_internal_ids(&one_line);
    let clean = clean.trim();
    if clean.is_empty() {
        None
    } else if clean.chars().count() > 80 {
        let cut = clean.chars().take(79).collect::<String>();
        Some(format!("{}…", cut.trim_end()))
    } else {
        Some(clean.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{close_sentence, close_title, single_row, synthesis_prompt, CloseOutcome};
    use excubia_chat::SynthesisRow;

    fn row(id: &str) -> SynthesisRow {
        SynthesisRow {
            delegation_id: id.to_string(),
            conversation_id: "conv".to_string(),
            result_message_id: "msg".to_string(),
        }
    }

    #[test]
    fn one_finished_delegation_is_closed_by_the_host_and_two_go_to_alfred() {
        assert!(single_row(&[row("dlg_a")]).is_some());
        assert!(single_row(&[row("dlg_a"), row("dlg_b")]).is_none());
        assert!(single_row(&[]).is_none());
    }

    #[test]
    fn a_cut_title_is_not_followed_by_a_second_stop() {
        let cut = "Muestra el contenido de README.md y…";
        assert_eq!(
            close_sentence("Ernesto", cut, CloseOutcome::Done, None),
            "Ernesto terminó: Muestra el contenido de README.md y…"
        );
        assert_eq!(
            close_sentence("Ernesto", cut, CloseOutcome::Denied, None),
            "No se hizo: Muestra el contenido de README.md y… Lo denegaste."
        );
    }

    #[test]
    fn the_close_names_the_seat_and_the_task_and_nothing_internal() {
        let title = close_title("Revisa los contactos de mañana");
        let done = close_sentence("Ernesto", &title, CloseOutcome::Done, None);
        assert_eq!(done, "Ernesto terminó: Revisar contactos de mañana.");
        let failed = close_sentence("Ernesto", &title, CloseOutcome::Failed, None);
        assert_eq!(
            failed,
            "Ernesto no pudo terminar: Revisar contactos de mañana."
        );
        let denied = close_sentence("Ernesto", &title, CloseOutcome::Denied, None);
        assert_eq!(
            denied,
            "No se hizo: Revisar contactos de mañana. Lo denegaste."
        );
        for text in [&done, &failed, &denied] {
            assert!(!text.contains("dlg_"), "{text}");
            assert!(!text.contains('\\'), "{text}");
            assert!(!text.contains('/'), "{text}");
        }
    }

    #[test]
    fn a_denial_quotes_the_operators_words_in_one_short_line() {
        let title = "Borrar README.md";
        let said = close_sentence(
            "Ernesto",
            title,
            CloseOutcome::Denied,
            Some("No borres nada:\n lo necesito para la demo"),
        );
        assert_eq!(
            said,
            "No se hizo: Borrar README.md. Lo denegaste: «No borres nada: lo necesito para la demo»."
        );
        // Blank words are no reason; a long one is cut at 80 characters with «…».
        assert_eq!(
            close_sentence("Ernesto", title, CloseOutcome::Denied, Some("  ")),
            "No se hizo: Borrar README.md. Lo denegaste."
        );
        let long = "palabra ".repeat(20);
        let cut = close_sentence("Ernesto", title, CloseOutcome::Denied, Some(&long));
        let quoted = cut.split('«').nth(1).unwrap().trim_end_matches("».");
        assert_eq!(quoted.chars().count(), 80);
        assert!(quoted.ends_with('…'));
        // The operator's ids never reach Alfred's sentence.
        let ids = close_sentence(
            "Ernesto",
            title,
            CloseOutcome::Denied,
            Some("no lo toques (tarea dlg_01hzx7k2m3n4p5q6r7s8t9v0wa)"),
        );
        assert!(!ids.contains("dlg_"), "{ids}");
    }

    #[test]
    fn a_path_in_the_task_keeps_only_the_file_name() {
        let title = close_title("Mueve C:\\Users\\Max\\notas.txt a src/docs/");
        assert!(!title.contains('\\') && !title.contains('/'), "{title}");
        assert!(title.contains("notas.txt"), "{title}");
    }

    #[test]
    fn a_task_without_readable_words_falls_back_to_its_first_forty_characters() {
        let task = "x".repeat(60);
        assert_eq!(close_title(&task).chars().count(), 40);
    }

    #[test]
    fn the_synthesis_turn_asks_for_one_short_sentence_without_ids() {
        let text = synthesis_prompt(&["- Ana → Lead: informe => Listo".to_string()], false);
        assert!(text.contains("UNA frase"));
        assert!(text.contains("nunca la ruta"));
        assert!(!text.contains("dlg_"));
        assert!(text.contains("- Ana → Lead: informe => Listo"));
        assert!(!text.contains("El usuario denegó"));
    }

    #[test]
    fn a_question_or_a_look_asks_for_the_answer_and_an_order_does_not() {
        use super::{asks_for_answer, excerpt};
        for task in [
            "Solo lectura, no cambies nada: indica en qué rama está el proyecto",
            "Dime cuántos archivos hay",
            "Solo consulta.",
        ] {
            assert!(asks_for_answer(task), "{task}");
        }
        for task in ["Revisa contactos de mañana", "Crea redi.txt con el texto uno"] {
            assert!(!asks_for_answer(task), "{task}");
        }
        assert_eq!(excerpt("## Estado\n\n**Rama:** master").as_str(), "## Estado **Rama:** master");
        assert_eq!(excerpt(&"a".repeat(900)).chars().count(), 700);
    }

    #[test]
    fn a_denied_action_is_said_as_not_done_without_offering_to_insist() {
        let text = synthesis_prompt(&["- Ana → Lead: mover => denegada".to_string()], true);
        assert!(text.contains("El usuario denegó la acción"));
        assert!(text.contains("No preguntes si quieres insistir"));
    }
}
