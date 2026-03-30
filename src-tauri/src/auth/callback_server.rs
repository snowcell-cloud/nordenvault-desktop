use anyhow::{anyhow, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

pub const DEV_CALLBACK_PORT: u16 = 51234;
pub const DEV_REDIRECT_URI: &str = "http://127.0.0.1:51234/callback";

/// Starts a one-shot HTTP server, waits for the OAuth callback,
/// returns (code, state). Closes after the first request.
pub async fn wait_for_callback() -> Result<(String, String)> {
    let listener =
        TcpListener::bind(format!("127.0.0.1:{}", DEV_CALLBACK_PORT)).await?;

    let (stream, _) = listener.accept().await?;
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    // Read only the request line: "GET /callback?code=...&state=... HTTP/1.1"
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;

    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("Bad HTTP request"))?;

    let url = url::Url::parse(&format!("http://127.0.0.1{}", path))?;

    let mut code = None;
    let mut state = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.to_string()),
            "state" => state = Some(v.to_string()),
            _ => {}
        }
    }

    // Respond with a success page and try to close the tab
    let html = r#"<!DOCTYPE html>
<html>
<head><title>NordenVault</title>
<style>body{font-family:-apple-system,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#f7f8fa;}
.box{text-align:center;padding:40px;}.icon{font-size:48px;}.title{font-size:20px;font-weight:600;margin:16px 0 8px;}.sub{color:#6b7280;}</style>
</head>
<body><div class="box">
<div class="icon">✓</div>
<div class="title">Login successful</div>
<div class="sub">You can close this tab and return to NordenVault.</div>
</div>
<script>setTimeout(()=>window.close(),1000);</script>
</body></html>"#;

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    writer.write_all(response.as_bytes()).await?;
    writer.flush().await?;

    match (code, state) {
        (Some(c), Some(s)) => Ok((c, s)),
        _ => Err(anyhow!("Missing code or state in callback")),
    }
}
