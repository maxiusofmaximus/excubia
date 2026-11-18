//! What Alfred tells the user carries no internal identifier (REDI-ALFRED2): the reply
//! and the question of a turn are stored, and sent on, without the `dlg_…` or `apr_…`
//! the tool results gave it.

use excubia_chat::{ChatStore, NewConversation};

const TASK: &str = "dlg_01M7JMD9JNE1NYT6WNFVCGEXBD";

fn turn(store: &ChatStore) -> (String, String) {
    let conversation_id = store
        .create_conversation(NewConversation {
            title: Some("Pruebas".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id;
    let accepted = store
        .append_user_message(&conversation_id, "u-1", "hazlo", None)
        .expect("message");
    (conversation_id, accepted.turn.id)
}

#[test]
fn a_reply_is_stored_without_the_internal_ids_it_was_given() {
    let store = ChatStore::in_memory().expect("chat");
    let (_, run_id) = turn(&store);

    let reply = store
        .complete_run_with_files(&run_id, &format!("Quedó en cola (tarea {TASK})."), &[], &[])
        .expect("reply");

    assert_eq!(reply.body_md, "Quedó en cola.");
}

#[test]
fn a_question_is_stored_without_the_internal_ids_it_was_given() {
    let store = ChatStore::in_memory().expect("chat");
    let (_, run_id) = turn(&store);

    let question = store
        .append_agent_question(&run_id, &format!("¿Lo hago? {TASK} espera."))
        .expect("question");

    assert_eq!(question.body_md, "¿Lo hago? espera.");
}
