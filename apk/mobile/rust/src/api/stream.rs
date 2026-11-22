//! Stateful SSE decoding for the Dart transport: Dart owns the socket and the
//! line loop, Rust owns framing, JSON and the `state_change_seq` continuity.

use crate::sse::{RawFrame, SseParser};
use crate::state_wire::{revision_of, state_change_seq_of};

use super::chat::{parse_chat_event, ChatEvent};
use super::state::{parse_sse_state, StateSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEventKind {
    State,
    Change,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamEvent {
    pub event: String,
    pub kind: StreamEventKind,
    pub revision: Option<u32>,
    pub state_change_seq: Option<u32>,
    pub seq_gap: bool,
    pub state: Option<StateSnapshot>,
    /// A decoded `chat.*` frame. Its body never carries a message body, only
    /// the ids and the preview; the controller refreshes the timeline.
    pub chat: Option<ChatEvent>,
}

#[flutter_rust_bridge::frb(opaque)]
pub struct StreamDecoder {
    parser: SseParser,
    last_seq: Option<u32>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn new_stream_decoder() -> StreamDecoder {
    StreamDecoder {
        parser: SseParser::default(),
        last_seq: None,
    }
}

impl StreamDecoder {
    /// Feed one line. A completed frame is decoded; a comment or a malformed
    /// state body yields nothing so the caller keeps its last good state.
    #[flutter_rust_bridge::frb(sync)]
    pub fn push_line(&mut self, line: String) -> Option<StreamEvent> {
        let frame = self.parser.accept(&line)?;
        decode_frame(frame, &mut self.last_seq)
    }
}

fn decode_frame(frame: RawFrame, last_seq: &mut Option<u32>) -> Option<StreamEvent> {
    if frame.event == "state" {
        let state = parse_sse_state(&frame.data)?;
        let revision = revision_of(&frame.data).or(Some(state.revision));
        return Some(StreamEvent {
            event: frame.event,
            kind: StreamEventKind::State,
            revision,
            state_change_seq: state_change_seq_of(&frame.data),
            seq_gap: false,
            state: Some(state),
            chat: None,
        });
    }

    let revision = revision_of(&frame.data);
    if frame.event.starts_with("chat.") {
        let chat = parse_chat_event(frame.data)?;
        return Some(StreamEvent {
            event: frame.event,
            kind: StreamEventKind::Change,
            revision,
            state_change_seq: None,
            seq_gap: false,
            state: None,
            chat: Some(chat),
        });
    }

    let seq = state_change_seq_of(&frame.data);
    let seq_gap = matches!(
        (seq, *last_seq),
        (Some(seq), Some(last)) if seq > last.saturating_add(1)
    );
    if let Some(seq) = seq {
        *last_seq = Some((*last_seq).map_or(seq, |last| last.max(seq)));
    }
    Some(StreamEvent {
        event: frame.event,
        kind: StreamEventKind::Change,
        revision,
        state_change_seq: seq,
        seq_gap,
        state: None,
        chat: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(decoder: &mut StreamDecoder, lines: &[&str]) -> Vec<StreamEvent> {
        lines
            .iter()
            .filter_map(|line| decoder.push_line((*line).to_string()))
            .collect()
    }

    #[test]
    fn a_state_frame_carries_the_full_snapshot() {
        let mut decoder = new_stream_decoder();
        let events = push(
            &mut decoder,
            &[
                "event: state",
                r#"data: {"event":"state","revision":7,"state":{"revision":7,"agents":[{"pane_id":"w1:p1"}]}}"#,
                "",
            ],
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, StreamEventKind::State);
        assert_eq!(events[0].revision, Some(7));
        assert_eq!(events[0].state.as_ref().unwrap().agents[0].pane_id, "w1:p1");
    }

    #[test]
    fn a_change_frame_carries_revision_and_seq() {
        let mut decoder = new_stream_decoder();
        let events = push(
            &mut decoder,
            &[
                "event: status",
                r#"data: {"event":"status","revision":8,"state_change_seq":4}"#,
                "",
            ],
        );
        assert_eq!(events[0].kind, StreamEventKind::Change);
        assert_eq!(events[0].revision, Some(8));
        assert_eq!(events[0].state_change_seq, Some(4));
        assert!(!events[0].seq_gap);
    }

    #[test]
    fn a_seq_hole_is_reported_but_the_frame_still_arrives() {
        let mut decoder = new_stream_decoder();
        let events = push(
            &mut decoder,
            &[
                "event: status",
                r#"data: {"revision":8,"state_change_seq":3}"#,
                "",
                "event: status",
                r#"data: {"revision":9,"state_change_seq":7}"#,
                "",
                "event: status",
                r#"data: {"revision":10,"state_change_seq":8}"#,
                "",
            ],
        );
        assert_eq!(events.len(), 3);
        assert!(
            !events[0].seq_gap,
            "no baseline: first seq cannot be a hole"
        );
        assert!(events[1].seq_gap, "7 after 3 missed three transitions");
        assert!(!events[2].seq_gap, "8 follows 7");
    }

    #[test]
    fn an_out_of_order_seq_does_not_look_like_a_hole() {
        let mut decoder = new_stream_decoder();
        let events = push(
            &mut decoder,
            &[
                "event: status",
                r#"data: {"revision":8,"state_change_seq":6}"#,
                "",
                "event: status",
                r#"data: {"revision":7,"state_change_seq":5}"#,
                "",
                "event: status",
                r#"data: {"revision":9,"state_change_seq":7}"#,
                "",
            ],
        );
        assert!(!events[1].seq_gap);
        assert!(!events[2].seq_gap, "7 follows the 6 already seen");
    }

    #[test]
    fn frames_without_seq_still_arrive() {
        let mut decoder = new_stream_decoder();
        let events = push(
            &mut decoder,
            &["event: updated", r#"data: {"revision":11}"#, ""],
        );
        assert_eq!(events[0].revision, Some(11));
        assert_eq!(events[0].state_change_seq, None);
        assert!(!events[0].seq_gap);
    }

    #[test]
    fn keepalives_and_malformed_state_frames_do_not_emit() {
        let mut decoder = new_stream_decoder();
        assert!(push(&mut decoder, &[": keepalive", ""]).is_empty());
        assert!(push(&mut decoder, &["event: state", "data: not json", ""]).is_empty());
    }

    #[test]
    fn a_chat_frame_carries_ids_and_no_state_snapshot() {
        let mut decoder = new_stream_decoder();
        let events = push(
            &mut decoder,
            &[
                "event: chat.message",
                r#"data: {"event":"chat.message","revision":58,"conversation_id":"cnv_1","message_id":"msg_1","seq":12,"change":"created","preview":"hola"}"#,
                "",
            ],
        );
        assert_eq!(events.len(), 1);
        assert!(events[0].state.is_none());
        let chat = events[0].chat.as_ref().unwrap();
        assert_eq!(chat.conversation_id, "cnv_1");
        assert_eq!(chat.seq, Some(12));
    }
}
