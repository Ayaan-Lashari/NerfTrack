// Derived from the pinned ccusage Codex adapter (MIT), commit
// 31e084afbca3981af97ab6b55abe4f38f451bad4. Nerfify retains only parser behavior.
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;

#[derive(Debug, Clone, Default)]
pub struct UsageEvent {
    pub timestamp_ms: i64,
    pub model: String,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub request_id: Option<String>,
    pub turn_id: Option<String>,
    pub session_id: Option<String>,
    pub explicit_provider: Option<String>,
    pub explicit_backend: Option<String>,
    pub authenticated_official_codex: bool,
    pub custom_alias: bool,
    pub long_context: bool,
    pub long_context_multiplier: f64,
    pub fast_mode: bool,
    pub fast_mode_multiplier: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParseStats {
    pub imported_records: u64,
    pub partial_line_retries: u64,
    pub rejected_records: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParserState {
    pub active_model: Option<String>,
    pub active_provider: Option<String>,
    pub active_backend: Option<String>,
    pub active_session_id: Option<String>,
    pub cumulative: BTreeMap<String, CumulativeTotals>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CumulativeTotals {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
}

fn get_u64(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(|value| {
            value.as_u64().or_else(|| {
                value
                    .as_f64()
                    .filter(|number| number.is_finite() && *number >= 0.0)
                    .map(|number| number.round() as u64)
            })
        })
        .unwrap_or(0)
}

fn nested<'a>(value: &'a Value, paths: &[&[&str]]) -> Option<&'a Value> {
    paths.iter().find_map(|path| {
        path.iter()
            .try_fold(value, |current, key| current.get(*key))
    })
}

fn string_at(value: &Value, paths: &[&[&str]]) -> Option<String> {
    nested(value, paths)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn bool_at(value: &Value, paths: &[&[&str]]) -> bool {
    nested(value, paths)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn opaque_identifier(value: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"nerfify-parser-id:");
    digest.update(value.as_bytes());
    format!("id_{:x}", digest.finalize())
}

fn timestamp_ms(value: &Value) -> i64 {
    if let Some(number) =
        nested(value, &[&["timestamp_ms"], &["timestamp"], &["created_at"]]).and_then(Value::as_i64)
    {
        return if number < 10_000_000_000 {
            number * 1000
        } else {
            number
        };
    }
    if let Some(timestamp) =
        nested(value, &[&["timestamp"], &["created_at"]]).and_then(Value::as_str)
    {
        if let Ok(parsed) = timestamp.parse::<i64>() {
            return if parsed < 10_000_000_000 {
                parsed * 1000
            } else {
                parsed
            };
        }
        if let Ok(parsed) = DateTime::parse_from_rfc3339(timestamp) {
            return parsed.timestamp_millis();
        }
    }
    0
}

pub fn parse_jsonl_line(line: &str) -> Result<UsageEvent, String> {
    let mut state = ParserState::default();
    parse_jsonl_line_with_state(line, &mut state)?.ok_or_else(|| "context-only record".into())
}

pub fn parse_jsonl_line_with_state(
    line: &str,
    state: &mut ParserState,
) -> Result<Option<UsageEvent>, String> {
    let value: Value = serde_json::from_str(line).map_err(|_| "invalid JSON record".to_string())?;
    let kind = string_at(&value, &[&["type"], &["event_type"], &["kind"]])
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_turn_context = matches!(kind.as_str(), "turn_context" | "turn-context" | "context")
        || value.get("turn_context").is_some();
    let last_token_usage = nested(&value, &[&["payload", "info", "last_token_usage"]]);
    let total_token_usage = nested(&value, &[&["payload", "info", "total_token_usage"]]);
    let usage = nested(
        &value,
        &[
            &["usage"],
            &["token_usage"],
            &["tokens"],
            &["response", "usage"],
            &["payload", "usage"],
            &["payload", "token_usage"],
            &["payload", "info", "last_token_usage"],
            &["payload", "info", "total_token_usage"],
        ],
    );
    let model_from_record = string_at(
        &value,
        &[
            &["model"],
            &["model_name"],
            &["turn_context", "model"],
            &["context", "model"],
            &["payload", "model"],
            &["payload", "model_name"],
            &["payload", "turn_context", "model"],
        ],
    );
    let provider_from_record = string_at(
        &value,
        &[
            &["provider"],
            &["provider_id"],
            &["backend", "provider"],
            &["payload", "provider"],
            &["payload", "provider_id"],
            &["payload", "model_provider"],
            &["payload", "backend", "provider"],
        ],
    );
    let backend_from_record = string_at(
        &value,
        &[
            &["backend"],
            &["backend_id"],
            &["model_context", "backend"],
            &["payload", "backend"],
            &["payload", "backend_id"],
            &["payload", "model_context", "backend"],
        ],
    );
    let session_from_record = string_at(
        &value,
        &[
            &["session_id"],
            &["sessionId"],
            &["session", "id"],
            &["payload", "session_id"],
            &["payload", "sessionId"],
            &["payload", "session", "id"],
        ],
    );
    if let Some(model) = model_from_record.clone() {
        state.active_model = Some(model);
    }
    if let Some(provider) = provider_from_record.clone() {
        state.active_provider = Some(provider);
    }
    if let Some(backend) = backend_from_record.clone() {
        state.active_backend = Some(backend);
    }
    if let Some(session_id) = session_from_record.clone() {
        state.active_session_id = Some(opaque_identifier(&session_id));
    }
    if is_turn_context && usage.is_none() {
        return Ok(None);
    }
    let usage = usage.unwrap_or(&value);
    let model = model_from_record
        .clone()
        .or_else(|| state.active_model.clone())
        .unwrap_or_else(|| "unknown".into());
    let mut input_tokens = get_u64(usage, &["input_tokens", "input", "prompt_tokens", "prompt"]);
    let mut cached_input_tokens = get_u64(
        usage,
        &[
            "cached_input_tokens",
            "cache_read_input_tokens",
            "cache_read",
            "cached_input",
        ],
    );
    let mut output_tokens = get_u64(
        usage,
        &["output_tokens", "output", "completion_tokens", "completion"],
    );
    let mut reasoning_tokens = get_u64(
        usage,
        &["reasoning_tokens", "reasoning_output_tokens", "reasoning"],
    );
    let cumulative = bool_at(
        &value,
        &[
            &["cumulative"],
            &["is_cumulative"],
            &["payload", "cumulative"],
            &["payload", "is_cumulative"],
            &["usage", "cumulative"],
            &["usage", "is_cumulative"],
        ],
    ) || (last_token_usage.is_none() && total_token_usage.is_some());
    if cumulative {
        let key = session_from_record
            .as_deref()
            .map(opaque_identifier)
            .or_else(|| state.active_session_id.clone())
            .or_else(|| model_from_record.clone())
            .unwrap_or_else(|| "global".into());
        let current = CumulativeTotals {
            input_tokens,
            cached_input_tokens,
            output_tokens,
            reasoning_tokens,
        };
        let previous = state.cumulative.insert(key, current).unwrap_or_default();
        input_tokens = input_tokens.saturating_sub(previous.input_tokens);
        cached_input_tokens = cached_input_tokens.saturating_sub(previous.cached_input_tokens);
        output_tokens = output_tokens.saturating_sub(previous.output_tokens);
        reasoning_tokens = reasoning_tokens.saturating_sub(previous.reasoning_tokens);
    }
    if input_tokens == 0 && output_tokens == 0 && reasoning_tokens == 0 {
        return Err("record has no token measurement".into());
    }
    let provider = provider_from_record.or_else(|| state.active_provider.clone());
    let backend = backend_from_record.or_else(|| state.active_backend.clone());
    let authenticated_official_codex = value
        .get("authenticated_codex")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || value
            .get("official_openai_codex")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || backend
            .as_deref()
            .is_some_and(|item| item.eq_ignore_ascii_case("codex"))
            && provider
                .as_deref()
                .is_some_and(|item| item.eq_ignore_ascii_case("openai"));
    let fast_mode = bool_at(&value, &[&["fast_mode"], &["payload", "fast_mode"]]);
    let long_context = bool_at(&value, &[&["long_context"], &["payload", "long_context"]]);
    Ok(Some(UsageEvent {
        timestamp_ms: timestamp_ms(&value),
        model,
        input_tokens,
        cached_input_tokens,
        output_tokens,
        reasoning_tokens,
        request_id: string_at(
            &value,
            &[
                &["request_id"],
                &["requestId"],
                &["id"],
                &["payload", "request_id"],
                &["payload", "requestId"],
            ],
        ),
        turn_id: string_at(
            &value,
            &[
                &["turn_id"],
                &["turnId"],
                &["turn", "id"],
                &["payload", "turn_id"],
                &["payload", "turnId"],
                &["payload", "turn", "id"],
            ],
        ),
        session_id: session_from_record
            .as_deref()
            .map(opaque_identifier)
            .or_else(|| state.active_session_id.clone()),
        explicit_provider: provider,
        explicit_backend: backend,
        authenticated_official_codex,
        custom_alias: bool_at(&value, &[&["custom_alias"], &["payload", "custom_alias"]]),
        long_context,
        long_context_multiplier: nested(
            &value,
            &[
                &["long_context_multiplier"],
                &["payload", "long_context_multiplier"],
            ],
        )
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && *number > 0.0)
        .unwrap_or(1.0),
        fast_mode,
        fast_mode_multiplier: nested(
            &value,
            &[
                &["fast_mode_multiplier"],
                &["payload", "fast_mode_multiplier"],
            ],
        )
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && *number > 0.0)
        .unwrap_or(1.0),
    }))
}

pub fn event_fingerprint(event: &UsageEvent) -> String {
    if event.request_id.is_some() || event.turn_id.is_some() {
        let mut digest = Sha256::new();
        digest.update(b"nerfify-explicit-id:");
        digest.update(event.request_id.as_deref().unwrap_or("").as_bytes());
        digest.update([0]);
        digest.update(event.turn_id.as_deref().unwrap_or("").as_bytes());
        return format!("fingerprint:{:x}", digest.finalize());
    }
    let mut digest = Sha256::new();
    digest.update(event.timestamp_ms.to_le_bytes());
    digest.update(
        event
            .model
            .trim()
            .to_ascii_lowercase()
            .replace('/', "-")
            .as_bytes(),
    );
    digest.update(event.input_tokens.to_le_bytes());
    digest.update(event.cached_input_tokens.to_le_bytes());
    digest.update(event.output_tokens.to_le_bytes());
    digest.update(event.reasoning_tokens.to_le_bytes());
    format!("fingerprint:{:x}", digest.finalize())
}

pub fn parse_newline_terminated<R: Read>(
    reader: R,
) -> Result<(Vec<UsageEvent>, ParseStats), String> {
    let mut state = ParserState::default();
    parse_newline_terminated_with_state(reader, &mut state)
}

pub fn parse_newline_terminated_with_state<R: Read>(
    mut reader: R,
    state: &mut ParserState,
) -> Result<(Vec<UsageEvent>, ParseStats), String> {
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|_| "unable to read JSONL source".to_string())?;
    let mut stats = ParseStats::default();
    let mut records = Vec::new();
    let has_terminal_newline = bytes.last().is_some_and(|byte| *byte == b'\n');
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.split('\n').collect();
    let last_index = lines.len().saturating_sub(1);
    for (index, line) in lines.into_iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let is_last = index == last_index;
        if is_last && !has_terminal_newline {
            stats.partial_line_retries += 1;
            continue;
        }
        match parse_jsonl_line_with_state(line, state) {
            Ok(Some(event)) => {
                stats.imported_records += 1;
                records.push(event);
            }
            Ok(None) => {}
            Err(_) => stats.rejected_records += 1,
        }
    }
    Ok((records, stats))
}

pub fn read_newline_terminated<R: Read>(reader: R) -> Result<(Vec<String>, ParseStats), String> {
    let (events, stats) = parse_newline_terminated(reader)?;
    Ok((events.iter().map(event_fingerprint).collect(), stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_tokens_from_common_shapes() {
        let event = parse_jsonl_line(r#"{"timestamp":1735689600,"model":"gpt-5-codex","usage":{"input_tokens":10,"cache_read_input_tokens":5,"output_tokens":4}}"#).expect("event");
        assert_eq!(event.timestamp_ms, 1_735_689_600_000);
        assert_eq!(event.input_tokens, 10);
        assert_eq!(event.cached_input_tokens, 5);
        assert_eq!(event.output_tokens, 4);
    }

    #[test]
    fn ignores_partial_final_line() {
        let line = r#"{"model":"gpt-5-codex","usage":{"input_tokens":10,"output_tokens":4}}"#;
        let (_, stats) =
            read_newline_terminated(format!("{line}\n{line}x").as_bytes()).expect("read");
        assert_eq!(stats.imported_records, 1);
        assert_eq!(stats.partial_line_retries, 1);
    }

    #[test]
    fn explicit_ids_win_for_deduplication() {
        let event = parse_jsonl_line(r#"{"request_id":"r1","turn_id":"t1","model":"gpt-5-codex","usage":{"input_tokens":1,"output_tokens":1}}"#).expect("event");
        let fingerprint = event_fingerprint(&event);
        assert!(fingerprint.starts_with("fingerprint:"));
        assert!(!fingerprint.contains("r1"));
        assert!(!fingerprint.contains("t1"));
    }

    #[test]
    fn carries_turn_context_and_recovers_cumulative_deltas() {
        let mut state = ParserState::default();
        assert!(parse_jsonl_line_with_state(
            r#"{"type":"turn_context","payload":{"model":"gpt-5-codex","provider":"openai","backend":"codex","session_id":"s1"}}"#,
            &mut state
        )
        .expect("context")
        .is_none());
        let first = parse_jsonl_line_with_state(
            r#"{"timestamp":1735689600,"cumulative":true,"session_id":"s1","usage":{"input_tokens":10,"output_tokens":4}}"#,
            &mut state,
        )
        .expect("first")
        .expect("usage");
        let second = parse_jsonl_line_with_state(
            r#"{"timestamp":1735689660,"cumulative":true,"session_id":"s1","usage":{"input_tokens":16,"output_tokens":7}}"#,
            &mut state,
        )
        .expect("second")
        .expect("usage");
        assert_eq!(first.model, "gpt-5-codex");
        assert_eq!(second.input_tokens, 6);
        assert_eq!(second.output_tokens, 3);
        assert_eq!(second.explicit_backend.as_deref(), Some("codex"));
    }

    #[test]
    fn parses_codex_rollout_token_count_records() {
        let mut state = ParserState::default();
        assert!(parse_jsonl_line_with_state(
            r#"{"timestamp":"2026-07-11T12:09:00.915Z","type":"turn_context","payload":{"turn_id":"t1","model":"gpt-5-codex","model_provider":"openai"}}"#,
            &mut state,
        )
        .expect("context")
        .is_none());
        let event = parse_jsonl_line_with_state(
            r#"{"timestamp":"2026-07-11T12:09:01.915Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":8,"reasoning_output_tokens":3},"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":8,"reasoning_output_tokens":3}}}}"#,
            &mut state,
        )
        .expect("token count")
        .expect("usage");
        assert_eq!(event.timestamp_ms, 1_783_771_741_915);
        assert_eq!(event.model, "gpt-5-codex");
        assert_eq!(event.input_tokens, 100);
        assert_eq!(event.cached_input_tokens, 20);
        assert_eq!(event.reasoning_tokens, 3);
        assert_eq!(event.explicit_provider.as_deref(), Some("openai"));
    }
}
