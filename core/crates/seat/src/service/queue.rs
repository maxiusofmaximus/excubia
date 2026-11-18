//! The seat's line of turns. A conversation waits in at most one batch: the user's
//! messages that arrive while that batch waits join it, so they are answered together
//! by one turn. A running turn is never touched by a message that arrives.

use std::collections::VecDeque;
use std::sync::Mutex;

use excubia_chat::Turn;
use tokio::sync::Notify;

use super::lock;

/// The turns one CLI run answers, in the order they arrived.
pub(super) struct Batch {
    pub(super) turns: Vec<Turn>,
    /// Only a batch of the user's messages takes more turns: Alfred's own turns stand alone.
    user: bool,
    conversation_id: String,
}

pub(super) struct Queue {
    pending: Mutex<VecDeque<Batch>>,
    wake: Notify,
    capacity: usize,
}

impl Queue {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            pending: Mutex::new(VecDeque::new()),
            wake: Notify::new(),
            capacity: capacity.max(1),
        }
    }

    /// Add a turn. It joins the waiting batch of its conversation when it can, and a
    /// joined turn takes no place in the line. Returns false when the line is full.
    pub(super) fn push(&self, turn: Turn, user: bool) -> bool {
        let mut pending = lock(&self.pending);
        if user {
            if let Some(batch) = pending
                .iter_mut()
                .find(|batch| batch.user && batch.conversation_id == turn.conversation_id)
            {
                batch.turns.push(turn);
                return true;
            }
        }
        if pending.len() >= self.capacity {
            return false;
        }
        pending.push_back(Batch {
            conversation_id: turn.conversation_id.clone(),
            user,
            turns: vec![turn],
        });
        drop(pending);
        self.wake.notify_one();
        true
    }

    /// Wait for the oldest batch. Only the worker calls this, so one permit is enough.
    pub(super) async fn next(&self) -> Batch {
        loop {
            if let Some(batch) = lock(&self.pending).pop_front() {
                return batch;
            }
            self.wake.notified().await;
        }
    }
}

/// The text one turn answers: its messages in order, each under its number when there
/// are several, so the agent sees them as one turn.
pub(super) fn merged_text(turns: &[Turn]) -> String {
    match turns {
        [only] => only.text.clone(),
        _ => {
            let total = turns.len();
            turns
                .iter()
                .enumerate()
                .map(|(index, turn)| format!("[Mensaje {} de {total}]\n{}", index + 1, turn.text))
                .collect::<Vec<_>>()
                .join("\n\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use excubia_chat::RunState;

    fn turn(id: &str, conversation: &str, text: &str) -> Turn {
        Turn {
            id: id.to_string(),
            state: RunState::Queued,
            agent_id: "ag_1".to_string(),
            conversation_id: conversation.to_string(),
            message_id: format!("msg_{id}"),
            text: text.to_string(),
        }
    }

    #[tokio::test]
    async fn messages_that_arrive_during_a_turn_become_one_next_turn() {
        let queue = Queue::new(2);
        assert!(queue.push(turn("run_1", "cnv_1", "primero"), true));
        // The worker takes the first turn and starts it; the next three arrive meanwhile.
        let running = queue.next().await;
        assert_eq!(running.turns.len(), 1);
        assert!(queue.push(turn("run_2", "cnv_1", "b"), true));
        assert!(queue.push(turn("run_3", "cnv_1", "c"), true));
        assert!(queue.push(turn("run_4", "cnv_1", "d"), true));
        let next = queue.next().await;
        let ids: Vec<&str> = next.turns.iter().map(|turn| turn.id.as_str()).collect();
        assert_eq!(ids, ["run_2", "run_3", "run_4"]);
        assert_eq!(
            merged_text(&next.turns),
            "[Mensaje 1 de 3]\nb\n\n[Mensaje 2 de 3]\nc\n\n[Mensaje 3 de 3]\nd"
        );
    }

    #[tokio::test]
    async fn a_merged_message_takes_no_place_and_a_full_line_refuses_a_new_batch() {
        let queue = Queue::new(1);
        assert!(queue.push(turn("run_1", "cnv_1", "a"), true));
        assert!(!queue.push(turn("run_2", "cnv_2", "otra"), true));
        assert!(queue.push(turn("run_3", "cnv_1", "b"), true));
    }

    #[tokio::test]
    async fn another_conversation_and_alfred_own_turns_never_join_a_batch() {
        let queue = Queue::new(4);
        assert!(queue.push(turn("run_1", "cnv_1", "a"), true));
        assert!(queue.push(turn("run_2", "cnv_2", "b"), true));
        assert!(queue.push(turn("run_3", "cnv_1", "sintesis"), false));
        assert!(queue.push(turn("run_4", "cnv_1", "c"), true));
        let first = queue.next().await;
        assert_eq!(first.turns.len(), 2, "run_1 and run_4 of cnv_1");
        assert_eq!(queue.next().await.turns[0].id, "run_2");
        assert_eq!(queue.next().await.turns[0].id, "run_3");
    }

    #[test]
    fn one_message_keeps_its_text_as_it_is() {
        assert_eq!(merged_text(&[turn("run_1", "cnv_1", "hola")]), "hola");
    }
}
