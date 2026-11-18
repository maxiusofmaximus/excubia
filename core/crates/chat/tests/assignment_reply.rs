//! A turn of Alfred that assigned work is closed with the host's line, one per delegation,
//! unless the reply asks something or is long.

mod common;

use common::{team, Team};

/// Alfred's turn for the operator's message: its run id.
fn alfred_turn(t: &Team) -> String {
    t.store
        .append_user_message(&t.conversation, "c-turn", "Pídele a Ernesto que cree redi.txt", None)
        .expect("message")
        .turn
        .id
}

const THREE_SENTENCES: &str = "Hecho. Le encargué a Ernesto, líder del proyecto Demo, que cree redi.txt. \
    Ernesto ya lo recibió. Te aviso cuando termine.";

#[test]
fn one_assignment_is_closed_with_the_host_line() {
    let t = team("assignment-line");
    let run = alfred_turn(&t);
    t.assign(&t.alfred, &t.lead, "Cree el archivo redi.txt con el texto uno", &run, None)
        .expect("delegation");

    let closed = t
        .store
        .complete_run_with_reply(&run, THREE_SENTENCES, &[])
        .expect("close");
    assert_eq!(closed.body_md, "Pedido a Ernesto: Crear redi.txt.");
}

#[test]
fn a_reply_that_asks_a_question_is_kept() {
    let t = team("assignment-question");
    let run = alfred_turn(&t);
    t.assign(&t.alfred, &t.lead, "Cree el archivo redi.txt con el texto uno", &run, None)
        .expect("delegation");

    let question = "¿Quieres que lo cree en otra carpeta?";
    let closed = t
        .store
        .complete_run_with_reply(&run, question, &[])
        .expect("close");
    assert_eq!(closed.body_md, question);
}

#[test]
fn a_long_reply_is_kept() {
    let t = team("assignment-long");
    let run = alfred_turn(&t);
    t.assign(&t.alfred, &t.lead, "Cree el archivo redi.txt con el texto uno", &run, None)
        .expect("delegation");

    let long = "Hecho. ".repeat(100);
    let closed = t
        .store
        .complete_run_with_reply(&run, &long, &[])
        .expect("close");
    assert_eq!(closed.body_md, long);
}

#[test]
fn a_turn_without_delegation_is_kept() {
    let t = team("assignment-none");
    let run = alfred_turn(&t);

    let closed = t
        .store
        .complete_run_with_reply(&run, THREE_SENTENCES, &[])
        .expect("close");
    assert_eq!(closed.body_md, THREE_SENTENCES);
}

#[test]
fn two_assignments_are_two_lines() {
    let t = team("assignment-two");
    let run = alfred_turn(&t);
    // Alfred assigns one lead at a time: the first delegation ends before the second.
    let first = t
        .assign(&t.alfred, &t.lead, "Crea a.txt", &run, None)
        .expect("first delegation");
    t.store
        .fail_delegation(&first.id, "seat_unavailable")
        .expect("first ends");
    t.assign(&t.alfred, &t.lead, "Revisa los contactos", &run, None)
        .expect("second delegation");

    let closed = t
        .store
        .complete_run_with_reply(&run, THREE_SENTENCES, &[])
        .expect("close");
    assert_eq!(
        closed.body_md,
        "Pedido a Ernesto: Crear a.txt.\nPedido a Ernesto: Revisar contactos."
    );
}
