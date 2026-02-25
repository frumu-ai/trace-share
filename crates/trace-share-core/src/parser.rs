use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::{
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::Path,
};

use crate::models::{CanonicalEvent, EventMeta, ToolInfo};

pub fn parse_jsonl_file(path: &Path, source: &str) -> Result<Vec<CanonicalEvent>> {
    let (events, _) = parse_jsonl_file_from_offset(path, source, 0)?;
    Ok(events)
}

pub fn parse_source_file(
    path: &Path,
    source: &str,
    format: &str,
    parser_hint: Option<&str>,
) -> Result<Vec<CanonicalEvent>> {
    match format {
        "jsonl" => parse_jsonl_file(path, source),
        "json" => parse_json_file(path, source, parser_hint),
        "mixed" => parse_mixed_file(path, source, parser_hint),
        other => anyhow::bail!("unsupported source format: {other}"),
    }
}

pub fn parse_jsonl_file_from_offset(
    path: &Path,
    source: &str,
    start_offset: u64,
) -> Result<(Vec<CanonicalEvent>, u64)> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(start_offset))?;
    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown-session")
        .to_string();

    let mut out = Vec::new();
    let mut next_offset = start_offset;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break;
        }
        next_offset += bytes_read as u64;
        let line = line.trim_end_matches(['\n', '\r']).to_string();
        if line.trim().is_empty() {
            continue;
        }

        let value = match serde_json::from_str::<Value>(&line) {
            Ok(v) => v,
            Err(_) => {
                out.push(fallback_event(source, &session_id, line));
                continue;
            }
        };

        out.push(value_to_event(source, &session_id, value));
    }

    Ok((out, next_offset))
}

fn value_to_event(source: &str, session_id: &str, v: Value) -> CanonicalEvent {
    let ts = extract_ts(&v).unwrap_or_else(Utc::now);
    let payload = v.get("payload").unwrap_or(&v);
    let top_type = v.get("type").and_then(Value::as_str).unwrap_or_default();
    let kind = infer_kind(top_type, payload);
    let text = {
        let from_payload = extract_text(payload);
        if !from_payload.trim().is_empty() {
            from_payload
        } else {
            extract_text(&v)
        }
    };

    let tool_name = payload
        .get("tool")
        .and_then(|t| t.get("name").or_else(|| Some(t)))
        .or_else(|| payload.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let tool = tool_name.map(|name| ToolInfo {
        name,
        args_json: payload.get("args").map(|a| a.to_string()),
        result_json: payload.get("result").map(|r| r.to_string()),
    });

    CanonicalEvent {
        source: source.to_string(),
        session_id: session_id.to_string(),
        ts,
        kind,
        text,
        tool,
        meta: Some(EventMeta {
            cwd: payload
                .get("cwd")
                .or_else(|| v.get("cwd"))
                .and_then(Value::as_str)
                .map(str::to_string),
            repo: payload
                .get("repo")
                .or_else(|| v.get("repo"))
                .and_then(Value::as_str)
                .map(str::to_string),
            exit_code: payload
                .get("exit_code")
                .or_else(|| v.get("exit_code"))
                .and_then(Value::as_i64)
                .map(|n| n as i32),
            model: payload
                .get("model")
                .or_else(|| v.get("model"))
                .and_then(Value::as_str)
                .map(str::to_string),
            tags: Vec::new(),
        }),
    }
}

fn fallback_event(source: &str, session_id: &str, text: String) -> CanonicalEvent {
    CanonicalEvent {
        source: source.to_string(),
        session_id: session_id.to_string(),
        ts: Utc::now(),
        kind: "system".to_string(),
        text,
        tool: None,
        meta: None,
    }
}

fn extract_ts(v: &Value) -> Option<DateTime<Utc>> {
    let candidate = v
        .get("ts")
        .or_else(|| v.get("timestamp"))
        .or_else(|| v.get("time"))
        .and_then(Value::as_str)?;

    DateTime::parse_from_rfc3339(candidate)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn parse_json_file(
    path: &Path,
    source: &str,
    parser_hint: Option<&str>,
) -> Result<Vec<CanonicalEvent>> {
    let value = read_json_with_retry(path)?;
    if parser_hint == Some("tandem_v1") {
        return parse_tandem_v1(source, &value);
    }
    Ok(vec![value_to_event(
        source,
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown-session"),
        value,
    )])
}

fn parse_mixed_file(
    path: &Path,
    source: &str,
    parser_hint: Option<&str>,
) -> Result<Vec<CanonicalEvent>> {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext = ext.to_ascii_lowercase();
        if ext == "jsonl" || ext == "ndjson" {
            return parse_jsonl_file(path, source);
        }
        if ext == "json" {
            return parse_json_file(path, source, parser_hint);
        }
    }

    parse_json_file(path, source, parser_hint).or_else(|_| parse_jsonl_file(path, source))
}

fn read_json_with_retry(path: &Path) -> Result<Value> {
    let mut last_err: Option<serde_json::Error> = None;
    for attempt in 0..3 {
        let text = std::fs::read_to_string(path)?;
        match serde_json::from_str::<Value>(&text) {
            Ok(v) => return Ok(v),
            Err(e) => {
                if e.is_eof() && attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    last_err = Some(e);
                    continue;
                }
                return Err(e.into());
            }
        }
    }
    match last_err {
        Some(e) => Err(e.into()),
        None => anyhow::bail!("failed to parse json file"),
    }
}

fn parse_tandem_v1(source: &str, root: &Value) -> Result<Vec<CanonicalEvent>> {
    let mut out = Vec::new();
    let Some(map) = root.as_object() else {
        return Ok(out);
    };

    for (session_key, session) in map {
        let session_id = session
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(session_key)
            .to_string();
        let cwd = session
            .get("workspace_root")
            .or_else(|| session.get("directory"))
            .and_then(Value::as_str)
            .map(str::to_string);

        let messages = session
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for msg in messages {
            let role = msg.get("role").and_then(Value::as_str).unwrap_or("system");
            let kind = match role {
                "user" => "user_msg",
                "assistant" => "assistant_msg",
                _ => "system",
            }
            .to_string();

            let ts = msg
                .get("created_at")
                .and_then(Value::as_str)
                .and_then(parse_rfc3339)
                .or_else(|| {
                    session
                        .get("time")
                        .and_then(|t| t.get("updated").or_else(|| t.get("created")))
                        .and_then(Value::as_str)
                        .and_then(parse_rfc3339)
                })
                .unwrap_or_else(Utc::now);

            let text = extract_tandem_message_text(&msg);
            if text.trim().is_empty() {
                continue;
            }

            out.push(CanonicalEvent {
                source: source.to_string(),
                session_id: session_id.clone(),
                ts,
                kind,
                text,
                tool: None,
                meta: Some(EventMeta {
                    cwd: cwd.clone(),
                    repo: None,
                    exit_code: None,
                    model: session
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    tags: vec!["tandem_v1".to_string()],
                }),
            });
        }
    }

    Ok(out)
}

fn extract_tandem_message_text(msg: &Value) -> String {
    if let Some(parts) = msg.get("parts").and_then(Value::as_array) {
        let joined = parts
            .iter()
            .filter_map(|p| {
                let ptype = p.get("type").and_then(Value::as_str).unwrap_or("");
                if ptype == "text" || ptype == "input_text" || ptype == "output_text" {
                    return p.get("text").and_then(Value::as_str).map(str::to_string);
                }
                None
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !joined.trim().is_empty() {
            return joined;
        }
    }
    extract_text(msg)
}

fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn extract_text(v: &Value) -> String {
    for key in [
        "text",
        "message",
        "content",
        "delta",
        "output_text",
        "input",
    ] {
        if let Some(raw) = v.get(key) {
            let value = flatten_text(raw, 0);
            if !value.trim().is_empty() {
                return value;
            }
        }
    }
    if let Some(item) = v.get("item") {
        let value = flatten_text(item, 0);
        if !value.trim().is_empty() {
            return value;
        }
    }
    String::new()
}

fn infer_kind(top_type: &str, payload: &Value) -> String {
    if top_type == "response_item" && payload.get("type").and_then(Value::as_str) == Some("message")
    {
        return match payload.get("role").and_then(Value::as_str) {
            Some("user") => "user_msg".to_string(),
            Some("assistant") => "assistant_msg".to_string(),
            _ => "system".to_string(),
        };
    }
    if top_type == "event_msg" {
        return match payload.get("type").and_then(Value::as_str) {
            Some("user_message") => "user_msg".to_string(),
            Some("tool_call") => "tool_call".to_string(),
            Some("tool_result") => "tool_result".to_string(),
            Some("error") => "error".to_string(),
            _ => "system".to_string(),
        };
    }
    if let Some(kind) = payload
        .get("kind")
        .or_else(|| payload.get("type"))
        .and_then(Value::as_str)
    {
        return kind.to_string();
    }
    if top_type.is_empty() {
        "system".to_string()
    } else {
        top_type.to_string()
    }
}

fn flatten_text(v: &Value, depth: usize) -> String {
    if depth > 5 {
        return String::new();
    }
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            let parts = items
                .iter()
                .map(|item| flatten_text(item, depth + 1))
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>();
            parts.join(" ")
        }
        Value::Object(map) => {
            for key in ["text", "content", "value", "output_text", "message"] {
                if let Some(raw) = map.get(key) {
                    let value = flatten_text(raw, depth + 1);
                    if !value.trim().is_empty() {
                        return value;
                    }
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn extracts_nested_content_arrays() {
        let v = json!({
            "content": [
                {"type":"output_text","text":"hello"},
                {"type":"output_text","text":"world"}
            ]
        });
        let text = super::extract_text(&v);
        assert!(text.contains("hello"));
        assert!(text.contains("world"));
    }

    #[test]
    fn maps_payload_wrapped_user_message() {
        let v = json!({
            "timestamp":"2026-02-25T09:51:59.245Z",
            "type":"response_item",
            "payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello from user"}]}
        });
        let ev = super::value_to_event("codex_cli", "s", v);
        assert_eq!(ev.kind, "user_msg");
        assert!(ev.text.contains("hello from user"));
    }

    #[test]
    fn parses_tandem_v1_sessions_json() {
        let v = json!({
            "s-1": {
                "id":"s-1",
                "workspace_root":"/tmp/proj",
                "messages":[
                    {"role":"user","created_at":"2026-02-25T00:00:00Z","parts":[{"type":"text","text":"hello"}]},
                    {"role":"assistant","created_at":"2026-02-25T00:00:01Z","parts":[{"type":"text","text":"world"}]}
                ]
            }
        });
        let events = super::parse_tandem_v1("tandem_sessions", &v).expect("parse");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, "user_msg");
        assert_eq!(events[1].kind, "assistant_msg");
        assert!(events[0].text.contains("hello"));
        assert!(events[1].text.contains("world"));
    }
}
