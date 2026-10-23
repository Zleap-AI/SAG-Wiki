use super::tokens;
use crate::{models, types::{ContextSummary, ModelConfig}};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

const PROMPT: &str = include_str!("../../prompts/context-summary.md");

pub(super) struct Compacted {
    pub messages: Vec<Value>,
    pub summary: ContextSummary,
    pub retained_history: usize,
    pub input_tokens: Option<usize>,
    pub output_tokens: Option<usize>,
}

pub(super) async fn compact(
    client: &reqwest::Client, config: &ModelConfig, key: &str,
    messages: &[Value], recent: &[Value], question: &Value, history_count: usize,
    definitions: &[Value], budget: usize, timeout: u64, cancel: &CancellationToken,
) -> Result<Compacted> {
    // Keep a complete recent tool exchange when it fits, never an unmatched tool reply.
    let mut boundary = messages.iter().rposition(|item| item["role"] == "assistant" && item["tool_calls"].is_array())
        .unwrap_or(messages.len());
    let mut tail = messages[boundary..].to_vec();
    let mut recent = recent.to_vec();
    let mut current = question.clone();
    let mut question_summarized = false;
    let mut next;
    loop {
        next = vec![messages[0].clone(), json!({"role":"user","content":""})];
        next.extend(recent.clone());
        next.push(current.clone());
        next.extend(tail.clone());
        if tokens(&next, definitions) + budget / 4 <= budget { break; }
        if !recent.is_empty() {
            recent.clear();
        } else if !tail.is_empty() {
            boundary = messages.len();
            tail.clear();
        } else {
            ensure!(!question_summarized, "系统提示词、工具定义或查询范围超过上下文预算，请调整配置");
            let mut content: Value = serde_json::from_str(question["content"].as_str().unwrap())?;
            content["question"] = json!("根据上下文摘要继续完成用户当前的请求，遵守其中保留的要求。");
            current["content"] = json!(content.to_string());
            question_summarized = true;
        }
    }
    let allowance = (budget - tokens(&next, definitions) - 32).min(budget / 3);
    let (text, input_tokens, output_tokens) = summarize(
        client, config, key, &messages[1..boundary], budget, allowance, timeout, cancel,
    ).await.map_err(|error| {
        let detail = format!("上下文压缩失败，原始记录已保留：{error:#}");
        error.context(detail)
    })?;
    next[1] = summary_message(&text, true);
    ensure!(tokens(&next, definitions) <= budget, "模型生成的上下文摘要仍超过预算，原始记录已保留");
    Ok(Compacted {
        messages: next,
        summary: ContextSummary { text, covered_turns: history_count },
        retained_history: recent.len() / 2,
        input_tokens,
        output_tokens,
    })
}

pub(super) fn summary_message(text: &str, current_sources: bool) -> Value {
    json!({"role":"user","content":json!({"context_summary":text,"source_ids_are_current":current_sources}).to_string()})
}

async fn summarize(
    client: &reqwest::Client, config: &ModelConfig, key: &str,
    messages: &[Value], budget: usize, allowance: usize, timeout: u64, cancel: &CancellationToken,
) -> Result<(String, Option<usize>, Option<usize>)> {
    let transcript = messages.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    ensure!(!transcript.is_empty(), "没有可总结的上下文");
    let target = allowance.min(budget / 5).min(2048);
    let system = format!("{PROMPT}\n摘要目标长度不超过 {target} tokens。只返回摘要正文。");
    let mut summary = String::new();
    let mut offset = 0;
    let mut input_tokens = None;
    let mut output_tokens = None;
    while offset < transcript.len() {
        // Oversized histories are summarized in complete Unicode slices, without dropping any text.
        let remaining = &transcript[offset..];
        let mut request = vec![json!({"role":"system","content":system}), json!({"role":"user","content":""})];
        let mut low = 0;
        let mut high = remaining.chars().count();
        while low < high {
            let count = low + (high - low).div_ceil(2);
            let end = remaining.char_indices().nth(count).map_or(remaining.len(), |(index, _)| index);
            request[1]["content"] = json!(json!({"previous_summary":summary,"transcript_part":&remaining[..end]}).to_string());
            if tokens(&request, &[]) <= budget { low = count; } else { high = count - 1; }
        }
        ensure!(low > 0, "上下文预算不足以生成摘要，请提高预算");
        let end = remaining.char_indices().nth(low).map_or(remaining.len(), |(index, _)| index);
        request[1]["content"] = json!(json!({"previous_summary":summary,"transcript_part":&remaining[..end]}).to_string());
        for attempt in 0..2 {
            let payload = models::payload(config, request.clone(), true)?;
            let turn = models::chat_turn(client, config, key, &payload, timeout, cancel, |_| {}).await?;
            if let Some(n) = turn.input_tokens { input_tokens = Some(input_tokens.unwrap_or(0) + n); }
            if let Some(n) = turn.output_tokens { output_tokens = Some(output_tokens.unwrap_or(0) + n); }
            ensure!(turn.tool_calls.is_empty(), "摘要请求不能调用工具");
            ensure!(!turn.text.trim().is_empty(), "模型没有返回有效的上下文摘要");
            if models::estimate(&json!(turn.text)) <= allowance {
                summary = turn.text;
                break;
            }
            ensure!(attempt == 0, "模型生成的摘要超过预算，请重试或提高预算");
            request[1]["content"] = json!(json!({"previous_summary":turn.text,"instruction":format!("保留关键任务和约束，将这份摘要进一步压缩到 {target} tokens 内。")}).to_string());
            ensure!(tokens(&request, &[]) <= budget, "摘要超过模型可处理的上下文预算");
        }
        offset += end;
    }
    Ok((summary, input_tokens, output_tokens))
}
