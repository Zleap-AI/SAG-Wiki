use anyhow::{bail, Context, Result};
use tokio::{io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader}, net::TcpStream};

const SKILL: &str = include_str!("../../cli/skills/sag/SKILL.md");

// Only bundled, public instructions are exposed here; RPC keeps its token authentication.
pub async fn serve(mut stream: TcpStream) -> Result<()> {
    let request = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut reader = BufReader::new(&mut stream);
        let mut bytes = Vec::new();
        loop {
            let start = bytes.len();
            let count = (&mut reader).take((8193 - start) as u64).read_until(b'\n', &mut bytes).await?;
            if count == 0 { bail!("HTTP 请求未完成") }
            if bytes.len() > 8192 { bail!("HTTP 请求头超过上限") }
            if matches!(&bytes[start..], b"\r\n" | b"\n") { break; }
        }
        Ok::<_, anyhow::Error>(bytes)
    }).await.context("HTTP 请求超时")??;
    let line = std::str::from_utf8(request.split(|byte| *byte == b'\n').next().unwrap())?;
    let mut parts = line.split_whitespace();
    let (method, path, version) = (parts.next(), parts.next(), parts.next());
    let (status, body) = match (method, path, version) {
        (Some("GET" | "HEAD"), Some(path), Some("HTTP/1.0" | "HTTP/1.1")) => match path {
            "/sag-wiki.md" | "/SKILL.md" => ("200 OK", SKILL),
            "/references/setup.md" => ("200 OK", include_str!("../../cli/skills/sag/references/setup.md")),
            "/references/parameters.md" => ("200 OK", include_str!("../../cli/skills/sag/references/parameters.md")),
            "/references/ignores.md" => ("200 OK", include_str!("../../cli/skills/sag/references/ignores.md")),
            _ => ("404 Not Found", "Not found\n"),
        },
        (Some(_), Some(_), Some("HTTP/1.0" | "HTTP/1.1")) => ("405 Method Not Allowed", "Use GET or HEAD\n"),
        _ => ("400 Bad Request", "Bad request\n"),
    };
    let headers = format!("HTTP/1.1 {status}\r\nContent-Type: text/markdown; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n", body.len());
    stream.write_all(headers.as_bytes()).await?;
    if method != Some("HEAD") { stream.write_all(body.as_bytes()).await?; }
    stream.shutdown().await?;
    Ok(())
}
