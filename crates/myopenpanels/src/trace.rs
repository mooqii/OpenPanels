use crate::control::now_iso;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc;

const TRACE_LIMIT: usize = 500;
const TEXT_LIMIT: usize = 8000;
const RELEASE_TEXT_LIMIT: usize = 360;
const DEFAULT_RELEASE_SUMMARY: &str = "MyOpenPanels activity";

static TRACE_HUB: OnceLock<TraceHub> = OnceLock::new();

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceEventInput {
    pub audience: Option<String>,
    pub category: Option<String>,
    pub detail: Option<Value>,
    pub direction: Option<String>,
    pub release_summary: Option<String>,
    pub run_id: Option<String>,
    pub source: Option<String>,
    pub summary: Option<String>,
    pub task_id: Option<String>,
}

struct TraceHub {
    state: Mutex<TraceState>,
}

struct TraceState {
    events: VecDeque<Value>,
    next_seq: u64,
    subscribers: Vec<mpsc::UnboundedSender<Value>>,
}

impl TraceHub {
    fn new() -> Self {
        Self {
            state: Mutex::new(TraceState {
                events: VecDeque::with_capacity(TRACE_LIMIT),
                next_seq: 1,
                subscribers: Vec::new(),
            }),
        }
    }

    fn record(&self, input: TraceEventInput) -> Value {
        let mut state = self.state.lock().expect("trace state lock poisoned");
        let seq = state.next_seq;
        state.next_seq += 1;
        let event = normalize_event(seq, input);
        if state.events.len() >= TRACE_LIMIT {
            state.events.pop_front();
        }
        state.events.push_back(event.clone());
        state
            .subscribers
            .retain(|sender| sender.send(event.clone()).is_ok());
        event
    }

    fn snapshot(&self, audience: &str) -> Value {
        let state = self.state.lock().expect("trace state lock poisoned");
        let events = state
            .events
            .iter()
            .filter_map(|event| event_for_audience(event, audience))
            .collect::<Vec<_>>();
        json!({
            "events": events,
            "nextSeq": state.next_seq,
        })
    }

    fn subscribe(&self) -> mpsc::UnboundedReceiver<Value> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut state = self.state.lock().expect("trace state lock poisoned");
        state.subscribers.push(sender);
        receiver
    }

    fn clear(&self) {
        let mut state = self.state.lock().expect("trace state lock poisoned");
        state.events.clear();
    }
}

pub fn record(input: TraceEventInput) -> Value {
    hub().record(input)
}

pub fn record_simple(
    category: &str,
    source: &str,
    direction: Option<&str>,
    summary: impl Into<String>,
    release_summary: Option<String>,
    detail: Option<Value>,
) -> Value {
    record(TraceEventInput {
        audience: None,
        category: Some(category.to_owned()),
        detail,
        direction: direction.map(str::to_owned),
        release_summary,
        run_id: std::env::var("MYOPENPANELS_TRACE_RUN_ID").ok(),
        source: Some(source.to_owned()),
        summary: Some(summary.into()),
        task_id: None,
    })
}

pub fn snapshot(audience: &str) -> Value {
    hub().snapshot(audience)
}

pub fn subscribe() -> mpsc::UnboundedReceiver<Value> {
    hub().subscribe()
}

pub fn clear() {
    hub().clear()
}

pub fn event_for_audience(event: &Value, audience: &str) -> Option<Value> {
    if audience == "development" {
        return Some(event.clone());
    }
    let category = event.get("category").and_then(Value::as_str).unwrap_or("");
    let release_summary = event
        .get("releaseSummary")
        .and_then(Value::as_str)
        .or_else(|| event.get("summary").and_then(Value::as_str))
        .unwrap_or(DEFAULT_RELEASE_SUMMARY);
    if !is_release_visible_event(category, release_summary) {
        return None;
    }
    Some(json!({
        "id": event.get("id").cloned().unwrap_or(Value::Null),
        "seq": event.get("seq").cloned().unwrap_or(Value::Null),
        "timestamp": event.get("timestamp").cloned().unwrap_or(Value::Null),
        "category": category,
        "source": event.get("source").cloned().unwrap_or_else(|| json!("myopenpanels")),
        "direction": event.get("direction").cloned().unwrap_or(Value::Null),
        "summary": truncate_text(release_summary, RELEASE_TEXT_LIMIT),
        "releaseSummary": truncate_text(release_summary, RELEASE_TEXT_LIMIT),
        "taskId": event.get("taskId").cloned().unwrap_or(Value::Null),
    }))
}

pub fn emit_cli_event(input: TraceEventInput) {
    let Ok(url) = std::env::var("MYOPENPANELS_TRACE_URL") else {
        return;
    };
    let payload = normalize_external_event(input);
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_millis(600))
        .build();
    let _ = agent
        .post(&url)
        .set("content-type", "application/json")
        .send_json(payload);
}

pub fn trace_url_for_port(port: u16) -> String {
    format!("http://127.0.0.1:{port}/api/trace/events")
}

fn hub() -> &'static TraceHub {
    TRACE_HUB.get_or_init(TraceHub::new)
}

fn normalize_event(seq: u64, input: TraceEventInput) -> Value {
    let payload = normalize_external_event(input);
    let mut object = payload.as_object().cloned().unwrap_or_default();
    object.insert("id".to_owned(), json!(format!("trace:{seq}")));
    object.insert("seq".to_owned(), json!(seq));
    object
        .entry("timestamp".to_owned())
        .or_insert_with(|| json!(now_iso()));
    Value::Object(object)
}

fn normalize_external_event(input: TraceEventInput) -> Value {
    let category = normalize_category(input.category.as_deref());
    let summary = sanitize_text(
        input
            .summary
            .unwrap_or_else(|| "MyOpenPanels activity".to_owned()),
    );
    let release_summary = input
        .release_summary
        .map(sanitize_text)
        .unwrap_or_else(|| release_summary_for(&category, &summary));
    json!({
        "timestamp": now_iso(),
        "category": category,
        "source": sanitize_short(input.source.as_deref().unwrap_or("myopenpanels")),
        "direction": input.direction.map(|value| sanitize_short(&value)),
        "summary": summary,
        "releaseSummary": release_summary,
        "detail": sanitize_value(input.detail.unwrap_or(Value::Null), 0),
        "runId": input.run_id.or_else(|| std::env::var("MYOPENPANELS_TRACE_RUN_ID").ok()).map(|value| sanitize_short(&value)),
        "taskId": input.task_id.map(|value| sanitize_short(&value)),
    })
}

fn normalize_category(value: Option<&str>) -> String {
    let value = value.unwrap_or("system");
    match value {
        "agent" | "api" | "cli" | "error" | "system" | "task" => value.to_owned(),
        _ => "system".to_owned(),
    }
}

fn release_summary_for(category: &str, summary: &str) -> String {
    if matches!(category, "task" | "system" | "error") {
        truncate_text(summary, RELEASE_TEXT_LIMIT)
    } else {
        DEFAULT_RELEASE_SUMMARY.to_owned()
    }
}

fn is_release_visible_event(category: &str, release_summary: &str) -> bool {
    if matches!(category, "task" | "system" | "error") {
        return true;
    }

    matches!(category, "agent") && release_summary != DEFAULT_RELEASE_SUMMARY
}

fn sanitize_value(value: Value, depth: usize) -> Value {
    if depth > 6 {
        return json!("[truncated depth]");
    }
    match value {
        Value::String(value) => Value::String(sanitize_text(value)),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .take(80)
                .map(|value| sanitize_value(value, depth + 1))
                .collect(),
        ),
        Value::Object(object) => {
            let mut next = Map::new();
            for (key, value) in object.into_iter().take(120) {
                if is_sensitive_key(&key) {
                    next.insert(key, json!("[redacted]"));
                } else {
                    next.insert(key, sanitize_value(value, depth + 1));
                }
            }
            Value::Object(next)
        }
        other => other,
    }
}

fn sanitize_short(value: &str) -> String {
    truncate_text(value.replace(['\n', '\r'], " "), 160)
}

fn sanitize_text(value: String) -> String {
    let mut text = value;
    if looks_like_data_url(&text) || looks_like_base64(&text) {
        text = format!("[redacted large encoded content: {} chars]", text.len());
    }
    for marker in [
        "authorization",
        "api_key",
        "apikey",
        "access_token",
        "refresh_token",
        "password",
        "secret",
        "cookie",
    ] {
        text = redact_marker(&text, marker);
    }
    truncate_text(text, TEXT_LIMIT)
}

fn truncate_text(value: impl AsRef<str>, limit: usize) -> String {
    let value = value.as_ref();
    if value.chars().count() <= limit {
        return value.to_owned();
    }
    let head_len = limit.saturating_sub(80);
    let head = value.chars().take(head_len).collect::<String>();
    let tail = value
        .chars()
        .rev()
        .take(48)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!(
        "{head}\n[truncated: {} chars]\n{tail}",
        value.chars().count()
    )
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    [
        "authorization",
        "api-key",
        "apikey",
        "cookie",
        "password",
        "secret",
        "token",
        "access_token",
        "refresh_token",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn looks_like_data_url(value: &str) -> bool {
    value.starts_with("data:") && value.len() > 180
}

fn looks_like_base64(value: &str) -> bool {
    value.len() > 600
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '\n' | '\r'))
}

fn redact_marker(input: &str, marker: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let Some(index) = lower.find(marker) else {
        return input.to_owned();
    };
    let mut output = input[..index].to_owned();
    output.push_str(marker);
    output.push_str("=[redacted]");
    let rest_start = input[index..]
        .find(|ch: char| ch.is_whitespace() || matches!(ch, ',' | '&' | '}'))
        .map(|offset| index + offset)
        .unwrap_or(input.len());
    output.push_str(&input[rest_start..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn trace_ring_buffer_assigns_sequences() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear();
        let first = record(TraceEventInput {
            audience: None,
            category: Some("task".to_owned()),
            detail: None,
            direction: None,
            release_summary: Some("Started work".to_owned()),
            run_id: None,
            source: Some("test".to_owned()),
            summary: Some("Started work".to_owned()),
            task_id: None,
        });
        let second = record(TraceEventInput {
            audience: None,
            category: Some("task".to_owned()),
            detail: None,
            direction: None,
            release_summary: Some("Finished work".to_owned()),
            run_id: None,
            source: Some("test".to_owned()),
            summary: Some("Finished work".to_owned()),
            task_id: None,
        });
        assert!(second["seq"].as_u64().unwrap() > first["seq"].as_u64().unwrap());
        assert_eq!(
            snapshot("development")["events"].as_array().unwrap().len(),
            2
        );
    }

    #[test]
    fn trace_sanitizes_sensitive_payloads() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear();
        let event = record(TraceEventInput {
            audience: None,
            category: Some("cli".to_owned()),
            detail: Some(json!({
                "authorization": "Bearer secret-token",
                "body": format!("data:image/png;base64,{}", "a".repeat(700)),
                "nested": { "apiKey": "abc123" },
            })),
            direction: None,
            release_summary: None,
            run_id: None,
            source: Some("test".to_owned()),
            summary: Some("password=secret-token".to_owned()),
            task_id: None,
        });
        let serialized = serde_json::to_string(&event).unwrap();
        assert!(serialized.contains("[redacted]"));
        assert!(serialized.contains("redacted large encoded content"));
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("abc123"));
    }

    #[test]
    fn release_projection_hides_raw_debug_events() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear();
        record(TraceEventInput {
            audience: None,
            category: Some("cli".to_owned()),
            detail: Some(json!({ "stdout": "raw" })),
            direction: None,
            release_summary: None,
            run_id: None,
            source: Some("test".to_owned()),
            summary: Some("raw cli output".to_owned()),
            task_id: None,
        });
        record(TraceEventInput {
            audience: None,
            category: Some("agent".to_owned()),
            detail: Some(json!({ "stdout": "raw agent output" })),
            direction: Some("stdout".to_owned()),
            release_summary: None,
            run_id: None,
            source: Some("test".to_owned()),
            summary: Some("agent stdout raw".to_owned()),
            task_id: Some("task:raw".to_owned()),
        });
        record(TraceEventInput {
            audience: None,
            category: Some("agent".to_owned()),
            detail: Some(json!({ "stdin": "raw worker prompt" })),
            direction: Some("spawn".to_owned()),
            release_summary: Some("Started local agent worker".to_owned()),
            run_id: None,
            source: Some("test".to_owned()),
            summary: Some("Spawning local agent worker".to_owned()),
            task_id: Some("task:visible".to_owned()),
        });
        record(TraceEventInput {
            audience: None,
            category: Some("task".to_owned()),
            detail: Some(json!({ "prompt": "raw" })),
            direction: None,
            release_summary: Some("Document indexed".to_owned()),
            run_id: None,
            source: Some("test".to_owned()),
            summary: Some("raw task detail".to_owned()),
            task_id: None,
        });
        let snapshot = snapshot("release");
        let events = snapshot["events"].as_array().unwrap();
        assert_eq!(events.len(), 2);
        let serialized = serde_json::to_string(&events[0]).unwrap();
        assert!(serialized.contains("Started local agent worker"));
        assert!(!serialized.contains("raw worker prompt"));
        let serialized = serde_json::to_string(&events[1]).unwrap();
        assert!(serialized.contains("Document indexed"));
        assert!(!serialized.contains("prompt"));
        assert!(!serialized.contains("raw cli output"));
        assert!(!serialized.contains("raw agent output"));
    }
}
