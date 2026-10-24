use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

/// Reads one HTTP request: the head plus exactly `Content-Length` body bytes.
pub async fn read_request(socket: &mut TcpStream) -> (String, Vec<u8>) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let read = socket.read(&mut chunk).await.unwrap_or(0);
        assert!(read > 0, "client closed before sending headers");
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };

    let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let content_length: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())?
        })
        .unwrap_or(0);

    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let read = socket.read(&mut chunk).await.unwrap_or(0);
        assert!(read > 0, "client closed before sending the body");
        body.extend_from_slice(&chunk[..read]);
    }
    (headers, body)
}
