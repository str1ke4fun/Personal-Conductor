use anyhow::Context;
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct TranscriptMessage {
    pub role: String,
    pub text_preview: String,
    pub raw: Value,
}

pub async fn read_tail(transcript_path: &Path, n: usize) -> anyhow::Result<Vec<TranscriptMessage>> {
    let content = tokio::fs::read_to_string(transcript_path)
        .await
        .with_context(|| format!("read transcript {}", transcript_path.display()))?;
    let mut messages = Vec::new();
    for line in content.lines() {
        let Ok(raw) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let role = raw
            .get("role")
            .and_then(Value::as_str)
            .or_else(|| raw.pointer("/message/role").and_then(Value::as_str))
            .unwrap_or_default();
        if role != "assistant" {
            continue;
        }
        let Some(text) = extract_text(&raw) else {
            continue;
        };
        messages.push(TranscriptMessage {
            role: role.to_string(),
            text_preview: text.chars().take(80).collect(),
            raw,
        });
    }
    let start = messages.len().saturating_sub(n);
    Ok(messages.split_off(start))
}

fn extract_text(raw: &Value) -> Option<String> {
    for pointer in ["/text", "/message/content", "/content"] {
        if let Some(value) = raw.pointer(pointer) {
            if let Some(text) = value.as_str() {
                return Some(text.to_string());
            }
            if let Some(items) = value.as_array() {
                let joined = items
                    .iter()
                    .filter_map(|item| {
                        item.get("text")
                            .and_then(Value::as_str)
                            .or_else(|| item.get("content").and_then(Value::as_str))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !joined.is_empty() {
                    return Some(joined);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn read_tail_returns_last_n_assistant_text_previews() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("transcript.jsonl");
        let long_text = "a".repeat(100);
        let mut file = tokio::fs::File::create(&path)
            .await
            .expect("create transcript");
        file.write_all(br#"{"role":"user","text":"ignore me"}"#)
            .await
            .expect("write user");
        file.write_all(b"\nnot-json-yet\n")
            .await
            .expect("write invalid line");
        file.write_all(br#"{"role":"assistant","text":"first assistant"}"#)
            .await
            .expect("write assistant 1");
        file.write_all(b"\n").await.expect("write newline");
        file.write_all(
            format!(
                "{{\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}]}}}}\n",
                long_text
            )
            .as_bytes(),
        )
        .await
        .expect("write assistant 2");
        file.write_all(br#"{"role":"assistant","content":"third assistant"}"#)
            .await
            .expect("write assistant 3");
        file.write_all(b"\n").await.expect("write newline");
        file.flush().await.expect("flush transcript");

        let tail = read_tail(&path, 2).await.expect("read transcript tail");

        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].role, "assistant");
        assert_eq!(tail[0].text_preview.chars().count(), 80);
        assert_eq!(tail[1].text_preview, "third assistant");
    }
}
