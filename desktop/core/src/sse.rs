//! Minimal SSE decoder. It handles chunk boundaries, CRLF, comments and
//! multi-line `data:` fields; the gateway sends one JSON object per frame.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseMessage {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    event: Option<String>,
    data: String,
    has_data: bool,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw bytes from the HTTP body. Returns every complete frame.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseMessage> {
        self.buffer.extend_from_slice(chunk);
        let mut messages = Vec::new();

        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=newline).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line);
            self.line(&line, &mut messages);
        }

        messages
    }

    fn line(&mut self, line: &str, messages: &mut Vec<SseMessage>) {
        if line.is_empty() {
            if self.has_data || self.event.is_some() {
                messages.push(SseMessage {
                    event: self.event.take(),
                    data: std::mem::take(&mut self.data),
                });
            }
            self.has_data = false;
            self.event = None;
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => {
                if self.has_data {
                    self.data.push('\n');
                }
                self.data.push_str(value);
                self.has_data = true;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_one_frame() {
        let mut parser = SseParser::new();
        let messages = parser.feed(b"event: state\ndata: {\"revision\":1}\n\n");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].event.as_deref(), Some("state"));
        assert_eq!(messages[0].data, "{\"revision\":1}");
    }

    #[test]
    fn survives_chunks_splitting_lines_and_utf8() {
        let mut parser = SseParser::new();
        let payload = "event: state\ndata: {\"title\":\"diseño\"}\n\n";
        let bytes = payload.as_bytes();
        let mut messages = Vec::new();
        for chunk in bytes.chunks(3) {
            messages.extend(parser.feed(chunk));
        }
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].data, "{\"title\":\"diseño\"}");
    }

    #[test]
    fn ignores_keepalive_comments_and_handles_crlf() {
        let mut parser = SseParser::new();
        assert!(parser.feed(b": keepalive\r\n\r\n").is_empty());
        let messages = parser.feed(b"event: change\r\ndata: {\"revision\":9}\r\n\r\n");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].event.as_deref(), Some("change"));
        assert_eq!(messages[0].data, "{\"revision\":9}");
    }

    #[test]
    fn joins_multiline_data_and_ignores_unknown_fields() {
        let mut parser = SseParser::new();
        let messages = parser.feed(b"id: 7\nevent: note\ndata: uno\ndata:dos\n\n");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].data, "uno\ndos");
    }

    #[test]
    fn decodes_two_frames_in_one_chunk() {
        let mut parser = SseParser::new();
        let messages = parser.feed(
            b"event: state\ndata: {\"revision\":1}\n\nevent: change\ndata: {\"revision\":2}\n\n",
        );
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].event.as_deref(), Some("change"));
    }
}
