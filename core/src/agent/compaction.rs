use super::tokens;
use crate::{models, types::{ContextSummary, ModelConfig}};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

// Each summary request stays inside the chat budget; larger parts mean fewer
// sequential round trips for a long transcript.
const MAX_SUMMARY_INPUT: usize = 32_768;
const FALLBACK_NOTICE: &str = "本次未能生成模型摘要。较早的对话内容已省略，仅保留最近一轮对话和本轮记录；不要假设被省略内容的细节，需要时向用户确认。";
const CARRIED_SUMMARY: &str = "\n此前生成的摘要仍然有效：\n";

// Start before exhaustion and leave room for the next tool exchange.
pub(super) fn trigger_budget(budget: usize) -> usize { budget - budget / 10 }

pub(super) fn should_compact(messages: &[Value], question: &Value, definitions: &[Value], budget: usize) -> bool {
    let used = tokens(messages, definitions);
    used > budget || (messages.len() > 2 && used > trigger_budget(budget)
        && tokens(&[messages[0].clone(), json!({"role":"user","content":""}), question.clone()], definitions) + budget / 4 <= trigger_budget(budget))
}

const PROMPT: &str = include_str!("../../prompts/context-summary.md");

pub(super) struct Compacted {
    pub messages: Vec<Value>,
    pub summary: Option<ContextSummary>,
    pub retained_history: usize,
    pub input_tokens: Option<usize>,
    pub output_tokens: Option<usize>,
}

struct Fitted {
    next: Vec<Value>,
    boundary: usize,
    retained_history: usize,
    compact_budget: usize,
}

// The current question is the last structured user message; only this turn's
// exchanges follow it. Earlier turns replay their own tool calls and must never
// be mistaken for the current exchange.
fn question_index(messages: &[Value]) -> usize {
    messages.iter().rposition(|item| {
        item["role"] == "user" && item["content"].as_str()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .is_some_and(|content| content.get("question").is_some())
    }).unwrap_or(messages.len())
}

// Decide what stays verbatim next to the summary slot: the recent turn and this
// turn's last complete tool exchange when they fit, never an unmatched tool reply.
fn fit(messages: &[Value], recent: &[Value], question: &Value, definitions: &[Value], budget: usize) -> Result<Fitted> {
    let start = question_index(messages);
    let mut boundary = messages[start..].iter()
        .rposition(|item| item["role"] == "assistant" && item["tool_calls"].is_array())
        .map_or(messages.len(), |index| start + index);
    let mut tail = messages[boundary..].to_vec();
    let mut recent = recent.to_vec();
    let mut current = question.clone();
    let mut question_summarized = false;
    let mut next;
    let base = tokens(&[messages[0].clone(), json!({"role":"user","content":""}), question.clone()], definitions);
    let compact_budget = if base + budget / 4 <= trigger_budget(budget) { trigger_budget(budget) } else { budget };
    loop {
        next = vec![messages[0].clone(), json!({"role":"user","content":""})];
        next.extend(recent.clone());
        next.push(current.clone());
        next.extend(tail.clone());
        if tokens(&next, definitions) + budget / 4 <= compact_budget { break; }
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
    Ok(Fitted { next, boundary, retained_history: usize::from(!recent.is_empty()), compact_budget })
}

pub(super) async fn compact(
    client: &reqwest::Client, config: &ModelConfig, key: &str,
    messages: &[Value], recent: &[Value], question: &Value, history_count: usize,
    definitions: &[Value], budget: usize, timeout: u64, runtime: &sag_protocol::runtime::RuntimeSettings, cancel: &CancellationToken,
    mut progress: impl FnMut(&str),
) -> Result<Compacted> {
    let Fitted { mut next, boundary, retained_history, compact_budget } = fit(messages, recent, question, definitions, budget)?;
    let allowance = (compact_budget - tokens(&next, definitions) - 32).min(budget / 5);
    let (text, input_tokens, output_tokens) = summarize(
        client, config, key, &messages[1..boundary], budget, allowance, timeout, runtime, cancel, &mut progress,
    ).await?;
    next[1] = summary_message(&text, true);
    ensure!(tokens(&next, definitions) <= budget, "模型生成的上下文摘要仍超过预算，原始记录已保留");
    Ok(Compacted {
        messages: next,
        summary: Some(ContextSummary { text, covered_turns: history_count }),
        retained_history,
        input_tokens,
        output_tokens,
    })
}

// Without a model summary, continue from the recent turn and this turn's records.
// Nothing is persisted, so the next turn tries a model summary again.
pub(super) fn fallback(
    messages: &[Value], recent: &[Value], question: &Value, definitions: &[Value], budget: usize,
) -> Result<Compacted> {
    let Fitted { mut next, retained_history, .. } = fit(messages, recent, question, definitions, budget)?;
    // A summary already in place (from an earlier turn or earlier in this turn) still
    // holds the user's constraints; keep it with the notice when it fits.
    let carried = messages.get(1).and_then(|item| item["content"].as_str())
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .and_then(|value| value.get("context_summary")?.as_str().map(str::to_owned))
        // An earlier fallback notice is not a summary; keep only the summary it carried.
        .and_then(|text| match text.strip_prefix(FALLBACK_NOTICE) {
            Some(rest) => rest.strip_prefix(CARRIED_SUMMARY).map(str::to_owned),
            None => Some(text),
        });
    if let Some(text) = carried {
        next[1] = summary_message(&format!("{FALLBACK_NOTICE}{CARRIED_SUMMARY}{text}"), false);
        if tokens(&next, definitions) <= budget {
            return Ok(Compacted { messages: next, summary: None, retained_history, input_tokens: None, output_tokens: None });
        }
    }
    next[1] = summary_message(FALLBACK_NOTICE, false);
    ensure!(tokens(&next, definitions) <= budget, "系统提示词、工具定义或查询范围超过上下文预算，请调整配置");
    Ok(Compacted { messages: next, summary: None, retained_history, input_tokens: None, output_tokens: None })
}

pub(super) fn summary_message(text: &str, current_sources: bool) -> Value {
    json!({"role":"user","content":json!({"context_summary":text,"source_ids_are_current":current_sources}).to_string()})
}

async fn summarize(
    client: &reqwest::Client, config: &ModelConfig, key: &str,
    messages: &[Value], budget: usize, allowance: usize, timeout: u64,
    runtime: &sag_protocol::runtime::RuntimeSettings, cancel: &CancellationToken,
    mut progress: impl FnMut(&str),
) -> Result<(String, Option<usize>, Option<usize>)> {
    let transcript = messages.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    ensure!(!transcript.is_empty(), "没有可总结的上下文");
    let target = allowance.min(budget / 5).min(2048);
    // Reserve output even when the chat output reservation was smaller than the summary.
    let request_budget = budget.min(MAX_SUMMARY_INPUT).saturating_sub(target + 32);
    let system = format!("{PROMPT}\n摘要目标长度不超过 {target} tokens。只返回摘要正文。");
    let mut summary = String::new();
    let mut offset = 0;
    let mut completed = 0;
    let mut input_tokens = None;
    let mut output_tokens = None;
    while offset < transcript.len() {
        // Checkpoint: summary + offset advance only after a complete, validated response.
        let remaining = &transcript[offset..];
        let mut request = vec![json!({"role":"system","content":system}), json!({"role":"user","content":""})];
        let mut low = 0;
        let remaining_chars = remaining.chars().count();
        let mut high = remaining_chars;
        while low < high {
            let count = low + (high - low).div_ceil(2);
            let end = remaining.char_indices().nth(count).map_or(remaining.len(), |(index, _)| index);
            request[1]["content"] = json!(json!({"previous_summary":summary,"transcript_part":&remaining[..end]}).to_string());
            if tokens(&request, &[]) <= request_budget { low = count; } else { high = count - 1; }
        }
        ensure!(low > 0, "上下文预算不足以生成摘要，请提高预算");
        let mut end = remaining.char_indices().nth(low).map_or(remaining.len(), |(index, _)| index);
        request[1]["content"] = json!(json!({"previous_summary":summary,"transcript_part":&remaining[..end]}).to_string());
        // Later parts hold about as many characters as this one; the total is an estimate.
        let total = completed + 1 + (remaining_chars - low).div_ceil(low);
        progress(&format!("正在压缩上下文：第 {}/{total} 块", completed + 1));
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(runtime.retry_total_secs);
        let mut failures = 0;
        let mut refinements = 0;
        loop {
            let payload = models::summary_payload(config, request.clone(), target)?;
            let result = tokio::select! {
                _ = cancel.cancelled() => return Err(anyhow::anyhow!("已停止")),
                result = tokio::time::timeout_at(deadline,
                    models::summary_turn(client, config, key, &payload, timeout, runtime, cancel)
                ) => result.map_err(|_| anyhow::anyhow!("上下文摘要请求超过总时间限制"))?,
            };
            let turn = match result {
                Ok(turn) => turn,
                Err(error) => {
                    if !cancel.is_cancelled() && models::summary_output_limited(&error) && refinements == 0 {
                        // A truncated draft may omit late facts: retry the original input,
                        // with a tighter target, rather than summarize the partial draft.
                        refinements += 1;
                        request[0]["content"] = json!(format!("{system}\n请进一步精简，将摘要控制在 {} tokens 内。", (target / 2).max(1)));
                        continue;
                    }
                    if cancel.is_cancelled() || failures >= runtime.retry_count || !models::retryable_summary_error(&error) {
                        return Err(error);
                    }
                    let delay = runtime.retry_base_secs.saturating_mul(1u64 << failures.min(10)).min(runtime.retry_max_secs);
                    failures += 1;
                    // On transport failure, retry only the current part, with less input.
                    // A length-refinement request must keep its own draft instead.
                    if refinements == 0 && failures > 1 {
                        let count = remaining[..end].chars().count();
                        if count > 1 {
                            end = remaining.char_indices().nth(count.div_ceil(2)).unwrap().0;
                            request[1]["content"] = json!(json!({"previous_summary":summary,"transcript_part":&remaining[..end]}).to_string());
                        }
                    }
                    progress(&format!("正在重试上下文压缩：第 {}/{total} 块，第 {failures} 次重试", completed + 1));
                    tokio::select! {
                        _ = cancel.cancelled() => return Err(anyhow::anyhow!("已停止")),
                        _ = tokio::time::sleep_until(deadline) => return Err(anyhow::anyhow!("上下文摘要请求超过总时间限制")),
                        _ = tokio::time::sleep(std::time::Duration::from_secs(delay)) => {},
                    }
                    continue;
                }
            };
            if let Some(n) = turn.input_tokens { input_tokens = Some(input_tokens.unwrap_or(0) + n); }
            if let Some(n) = turn.output_tokens { output_tokens = Some(output_tokens.unwrap_or(0) + n); }
            ensure!(turn.tool_calls.is_empty(), "摘要请求不能调用工具");
            ensure!(!turn.text.trim().is_empty(), "模型没有返回有效的上下文摘要");
            // The provider's count can include reasoning tokens and the local estimate can
            // overcount CJK text; the smaller one describes the summary that was returned.
            let local = models::estimate(&json!(turn.text));
            let produced = turn.output_tokens.filter(|n| *n > 0).map_or(local, |n| n.min(local));
            if produced <= allowance {
                summary = turn.text;
                break;
            }
            ensure!(refinements == 0, "模型生成的摘要超过预算，请重试或提高预算");
            refinements += 1;
            request[1]["content"] = json!(json!({"previous_summary":turn.text,"instruction":format!("保留关键任务和约束，将这份摘要进一步压缩到 {target} tokens 内。")}).to_string());
            ensure!(tokens(&request, &[]) <= request_budget, "摘要超过模型可处理的上下文预算");
        }
        offset += end;
        completed += 1;
    }
    Ok((summary, input_tokens, output_tokens))
}

#[cfg(test)]
mod tests;
