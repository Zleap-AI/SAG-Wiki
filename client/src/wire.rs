use anyhow::{bail, Context, Result};
use sag_protocol::{Frame, MAX_FRAME, MAX_MESSAGE};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Serialize, Deserialize)]
struct Part {
    data: String,
    last: bool,
}

pub async fn write(writer: &mut (impl AsyncWrite + Unpin), frame: &Frame) -> Result<()> {
    let data = serde_json::to_string(frame)?;
    if data.len() > MAX_MESSAGE {
        bail!("消息超过 64 MiB，请缩小文件或查询范围")
    }
    let mut offset = 0;
    while offset < data.len() {
        let mut end = (offset + 32 * 1024).min(data.len());
        while !data.is_char_boundary(end) {
            end -= 1;
        }
        let mut bytes = serde_json::to_vec(&Part {
            data: data[offset..end].into(),
            last: end == data.len(),
        })?;
        if bytes.len() > MAX_FRAME {
            bail!("通信帧超过上限")
        }
        bytes.push(b'\n');
        writer.write_all(&bytes).await?;
        offset = end;
    }
    writer.flush().await?;
    Ok(())
}

pub async fn read(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Option<Frame>> {
    let mut message = String::new();
    loop {
        let mut bytes = vec![];
        let mut limited = (&mut *reader).take((MAX_FRAME + 1) as u64);
        let read = limited.read_until(b'\n', &mut bytes);
        let count = if message.is_empty() {
            read.await?
        } else {
            tokio::time::timeout(std::time::Duration::from_secs(10), read)
                .await
                .context("分块传输超时")??
        };
        if count == 0 {
            if message.is_empty() {
                return Ok(None);
            }
            bail!("分块传输未完成，连接已断开")
        }
        if count > MAX_FRAME || !bytes.ends_with(b"\n") {
            bail!("通信帧不完整或超过上限")
        }
        let part: Part = serde_json::from_slice(&bytes).context("通信帧格式无效")?;
        if message.len() + part.data.len() > MAX_MESSAGE {
            bail!("消息超过 64 MiB")
        }
        message.push_str(&part.data);
        if part.last {
            return Ok(Some(
                serde_json::from_str(&message).context("通信消息格式无效")?,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sag_protocol::{types::Draft, Operation};
    #[tokio::test]
    async fn large_unicode_message_crosses_small_frames_without_truncation() {
        let text = "中文🙂<img src='data:image/png;base64,AAAA'>".repeat(400_000);
        let expected = text.clone();
        let frame = Frame::Request {
            id: "large".into(),
            operation: Operation::PutDraft {
                draft: Draft {
                    path: "/test.md".into(),
                    name: "test.md".into(),
                    body: text,
                    hash: "h".into(),
                    is_new: false,
                },
            },
        };
        let (mut writer, reader) = tokio::io::duplex(4096);
        let writing = tokio::spawn(async move {
            write(&mut writer, &frame).await.unwrap();
        });
        let frame = read(&mut tokio::io::BufReader::new(reader))
            .await
            .unwrap()
            .unwrap();
        let Frame::Request {
            operation: Operation::PutDraft { draft },
            ..
        } = frame
        else {
            panic!("wrong frame")
        };
        assert_eq!(draft.body, expected);
        writing.await.unwrap();
    }
    #[tokio::test]
    async fn oversized_frame_is_rejected() {
        let input = vec![b'x'; MAX_FRAME + 1];
        assert!(read(&mut tokio::io::BufReader::new(input.as_slice()))
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("上限"));
    }
}
