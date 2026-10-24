//! HTTP/1.1 framing for one exchange. The client always sends
//! `Connection: close`, so a response body ends at its announced length, at
//! the last chunk, or at EOF, whichever the head promised.

use std::io::{self, BufRead, Read, Write};

use crate::api::pinned::HeaderField;

const MAX_HEAD_BYTES: u64 = 16 * 1024;
const MAX_LINE_BYTES: u64 = 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Framing {
    Length(u64),
    Chunked,
    UntilClose,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Head {
    pub(crate) status: u16,
    pub(crate) retry_after: Option<u32>,
    pub(crate) framing: Framing,
}

/// The whole request leaves in one write: a server that reads once must see it
/// complete, and nothing is sent at all when a header is unsafe.
pub(crate) fn write_request(
    out: &mut impl Write,
    method: &str,
    target: &str,
    host: &str,
    headers: &[HeaderField],
    body: &[u8],
) -> io::Result<()> {
    if headers
        .iter()
        .any(|field| has_line_break(&field.name) || has_line_break(&field.value))
    {
        return Err(invalid("a header contains a line break"));
    }
    let mut request = Vec::with_capacity(256 + body.len());
    write!(
        request,
        "{method} {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n"
    )?;
    for field in headers {
        write!(
            request,
            "{name}: {value}\r\n",
            name = field.name,
            value = field.value
        )?;
    }
    if !body.is_empty() {
        write!(request, "Content-Length: {}\r\n", body.len())?;
    }
    request.extend_from_slice(b"\r\n");
    request.extend_from_slice(body);
    out.write_all(&request)?;
    out.flush()
}

pub(crate) fn read_head(reader: &mut impl BufRead) -> io::Result<Head> {
    let mut remaining = MAX_HEAD_BYTES;
    let status_line = read_line(reader, &mut remaining)?;
    let status = parse_status(&status_line)?;
    let mut length = None;
    let mut chunked = false;
    let mut retry_after = None;
    loop {
        let line = read_line(reader, &mut remaining)?;
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(invalid("a response header has no colon"));
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = Some(parse_number::<u64>(value)?),
            "transfer-encoding" => chunked = value.to_ascii_lowercase().contains("chunked"),
            "retry-after" => retry_after = value.parse().ok(),
            _ => {}
        }
    }
    let framing = match (chunked, length) {
        (true, _) => Framing::Chunked,
        (false, Some(length)) => Framing::Length(length),
        (false, None) => Framing::UntilClose,
    };
    Ok(Head {
        status,
        retry_after,
        framing,
    })
}

/// The response body, decoded on the fly so SSE lines arrive as they are sent.
pub(crate) struct Body<R> {
    inner: R,
    state: State,
}

enum State {
    Length(u64),
    /// Bytes left in the current chunk; zero means the next chunk size is due.
    Chunk(u64),
    UntilClose,
    Done,
}

impl<R: BufRead> Body<R> {
    pub(crate) fn new(inner: R, framing: Framing) -> Self {
        let state = match framing {
            Framing::Length(0) => State::Done,
            Framing::Length(length) => State::Length(length),
            Framing::Chunked => State::Chunk(0),
            Framing::UntilClose => State::UntilClose,
        };
        Self { inner, state }
    }
}

impl<R: BufRead> Read for Body<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        match self.state {
            State::Done => Ok(0),
            State::UntilClose => self.inner.read(buf),
            State::Length(left) => {
                let read = read_data(&mut self.inner, buf, left)?;
                let left = left - read as u64;
                self.state = if left == 0 {
                    State::Done
                } else {
                    State::Length(left)
                };
                Ok(read)
            }
            State::Chunk(0) => {
                let size = read_chunk_size(&mut self.inner)?;
                if size == 0 {
                    skip_trailers(&mut self.inner)?;
                    self.state = State::Done;
                    return Ok(0);
                }
                self.state = State::Chunk(size);
                self.read(buf)
            }
            State::Chunk(left) => {
                let read = read_data(&mut self.inner, buf, left)?;
                let left = left - read as u64;
                if left == 0 {
                    expect_line_end(&mut self.inner)?;
                }
                self.state = State::Chunk(left);
                Ok(read)
            }
        }
    }
}

fn read_data(reader: &mut impl Read, buf: &mut [u8], limit: u64) -> io::Result<usize> {
    let max = buf.len().min(usize::try_from(limit).unwrap_or(usize::MAX));
    match reader.read(&mut buf[..max])? {
        0 => Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the response body ended early",
        )),
        read => Ok(read),
    }
}

fn read_chunk_size(reader: &mut impl BufRead) -> io::Result<u64> {
    let mut remaining = MAX_LINE_BYTES;
    let line = read_line(reader, &mut remaining)?;
    let digits = line.split(';').next().unwrap_or_default().trim();
    u64::from_str_radix(digits, 16).map_err(|_| invalid("a chunk size is not hexadecimal"))
}

fn skip_trailers(reader: &mut impl BufRead) -> io::Result<()> {
    let mut remaining = MAX_HEAD_BYTES;
    while !read_line(reader, &mut remaining)?.is_empty() {}
    Ok(())
}

fn expect_line_end(reader: &mut impl BufRead) -> io::Result<()> {
    let mut remaining = MAX_LINE_BYTES;
    if read_line(reader, &mut remaining)?.is_empty() {
        Ok(())
    } else {
        Err(invalid("a chunk is not followed by a line break"))
    }
}

/// One line without its terminator. The budget is shared across a head so an
/// endless header section cannot grow the buffer without bound.
fn read_line(reader: &mut impl BufRead, remaining: &mut u64) -> io::Result<String> {
    let mut line = String::new();
    let read = reader.by_ref().take(*remaining).read_line(&mut line)?;
    *remaining -= read as u64;
    if !line.ends_with('\n') {
        return Err(invalid("a line is cut off or too long"));
    }
    line.truncate(line.trim_end_matches(['\r', '\n']).len());
    Ok(line)
}

fn parse_status(line: &str) -> io::Result<u16> {
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some(version), Some(code)) if version.starts_with("HTTP/1.") => parse_number(code),
        _ => Err(invalid("the status line is not HTTP/1.x")),
    }
}

fn parse_number<T: std::str::FromStr>(text: &str) -> io::Result<T> {
    text.parse()
        .map_err(|_| invalid("a number in the head is not valid"))
}

fn has_line_break(text: &str) -> bool {
    text.contains(['\r', '\n'])
}

fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn decode(raw: &[u8], framing: Framing) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        Body::new(Cursor::new(raw.to_vec()), framing).read_to_end(&mut out)?;
        Ok(out)
    }

    #[test]
    fn a_chunked_body_is_joined_across_chunk_sizes_and_trailers() {
        let raw = b"4;name=x\r\nab12\r\n3\r\n34}\r\n0\r\nX-Trailer: 1\r\n\r\n";
        assert_eq!(decode(raw, Framing::Chunked).expect("body"), b"ab1234}");
    }

    #[test]
    fn a_length_body_stops_at_its_length() {
        let raw = b"{}extra";
        assert_eq!(decode(raw, Framing::Length(2)).expect("body"), b"{}");
    }

    #[test]
    fn a_body_without_length_runs_to_close() {
        assert_eq!(
            decode(b"event: x\n", Framing::UntilClose).expect("body"),
            b"event: x\n"
        );
    }

    #[test]
    fn a_truncated_length_body_is_an_error() {
        assert!(decode(b"{", Framing::Length(2)).is_err());
    }

    #[test]
    fn a_malformed_chunk_size_is_an_error() {
        assert!(decode(b"zz\r\nab\r\n", Framing::Chunked).is_err());
    }

    #[test]
    fn the_head_yields_status_retry_after_and_framing() {
        let raw = b"HTTP/1.1 503 Busy\r\nRetry-After: 7\r\nContent-Length: 2\r\n\r\n";
        let head = read_head(&mut Cursor::new(&raw[..])).expect("head");
        assert_eq!(
            head,
            Head {
                status: 503,
                retry_after: Some(7),
                framing: Framing::Length(2),
            }
        );
    }

    #[test]
    fn a_chunked_head_wins_over_a_length() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nTransfer-Encoding: chunked\r\n\r\n";
        let head = read_head(&mut Cursor::new(&raw[..])).expect("head");
        assert_eq!(head.framing, Framing::Chunked);
    }

    #[test]
    fn plain_text_is_not_an_http_response() {
        assert!(read_head(&mut Cursor::new(&b"hello there\r\n\r\n"[..])).is_err());
    }

    #[test]
    fn a_header_line_break_is_refused_before_anything_is_written() {
        let headers = [HeaderField {
            name: "X-HN-Nonce".to_string(),
            value: "a\r\nX-Evil: 1".to_string(),
        }];
        let mut out = Vec::new();
        assert!(write_request(&mut out, "GET", "/", "h", &headers, &[]).is_err());
        assert!(out.is_empty());
    }

    #[test]
    fn a_request_carries_host_close_and_length_only_with_a_body() {
        let headers = [HeaderField {
            name: "X-HN-Device".to_string(),
            value: "hn-1".to_string(),
        }];
        let mut out = Vec::new();
        write_request(
            &mut out,
            "POST",
            "/v1/pair",
            "10.0.0.2:8443",
            &headers,
            b"{}",
        )
        .expect("write");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text
            .starts_with("POST /v1/pair HTTP/1.1\r\nHost: 10.0.0.2:8443\r\nConnection: close\r\n"));
        assert!(text.contains("X-HN-Device: hn-1\r\n"));
        assert!(text.ends_with("Content-Length: 2\r\n\r\n{}"));
    }
}
