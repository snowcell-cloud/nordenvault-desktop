use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

pub const REDIRECT_URI: &str = "http://127.0.0.1:51234/callback";

pub async fn wait_for_callback(listener: TcpListener, expected_state: &str) -> Result<String> {
    loop {
        let (mut socket, _) = listener.accept().await?;
        let mut request = String::new();
        let read = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            BufReader::new((&mut socket).take(8192)).read_line(&mut request),
        )
        .await;
        if !matches!(read, Ok(Ok(n)) if n > 0 && n < 8192) {
            continue;
        }
        let mut parts = request.split_whitespace();
        let code = (|| {
            if parts.next()? != "GET" {
                return None;
            }
            let query = parts.next()?.strip_prefix("/callback?")?;
            let fields: Vec<_> = url::form_urlencoded::parse(query.as_bytes()).collect();
            let mut states = fields.iter().filter(|(key, _)| key == "state");
            if states.next()?.1 != expected_state || states.next().is_some() {
                return None;
            }
            let mut codes = fields.iter().filter(|(key, _)| key == "code");
            let code = &codes.next()?.1;
            if code.is_empty() || codes.next().is_some() {
                return None;
            }
            Some(code.to_string())
        })();
        let response = if code.is_some() {
            "HTTP/1.1 200 OK\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Type: text/plain\r\n\r\nReturn to NordenVault to finish signing in."
        } else {
            "HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nInvalid callback."
        };
        let _ = socket.write_all(response.as_bytes()).await;
        if let Some(code) = code {
            return Ok(code);
        }
    }
}
