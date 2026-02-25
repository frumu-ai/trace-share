use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{models::CanonicalEvent, sanitize::contains_sensitive_patterns};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeOutcomeSignals {
    pub tests_passed: Option<bool>,
    pub exit_code: Option<i32>,
    pub lint_fixed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeOutcome {
    pub success: bool,
    pub signals: EpisodeOutcomeSignals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeMeta {
    pub lang: Option<String>,
    pub tool_names: Vec<String>,
    pub error_types: Vec<String>,
    pub repo_fingerprint: Option<String>,
    pub os: Option<String>,
    pub editor: Option<String>,
    pub raw_content_included: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeConsent {
    pub accepted_at: String,
    pub consent_version: String,
    pub public_searchable: bool,
    pub trainable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeStep {
    pub role: String,
    pub content: String,
    pub name: Option<String>,
    pub ts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeRecord {
    pub id: String,
    pub source_tool: String,
    pub session_id: String,
    pub ts_start: String,
    pub ts_end: String,
    pub prompt: String,
    pub context: String,
    pub trace: Vec<EpisodeStep>,
    pub result: String,
    pub outcome: EpisodeOutcome,
    pub meta: EpisodeMeta,
    pub consent: EpisodeConsent,
    pub license: String,
    pub policy_version: String,
    pub sanitizer_version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftRecord {
    pub id: String,
    pub instruction: String,
    pub input: String,
    pub output: String,
    pub meta: EpisodeMeta,
    pub license: String,
    pub consent: EpisodeConsent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TooltraceMessage {
    pub role: String,
    pub content: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TooltraceRecord {
    pub id: String,
    pub messages: Vec<TooltraceMessage>,
    pub meta: EpisodeMeta,
    pub license: String,
    pub consent: EpisodeConsent,
}

pub fn build_episode(
    source_tool: &str,
    session_id: &str,
    events: &[CanonicalEvent],
    include_raw: bool,
    accepted_at: &str,
    consent_version: &str,
    license: &str,
    policy_version: &str,
    sanitizer_version: &str,
) -> Option<EpisodeRecord> {
    if events.is_empty() {
        return None;
    }

    let raw_prompt = events
        .iter()
        .find(|e| e.kind == "user_msg")
        .map(|e| e.text.clone())
        .unwrap_or_default();

    let raw_result = events
        .iter()
        .rev()
        .find(|e| e.kind == "assistant_msg" || e.kind == "response_item")
        .map(|e| e.text.clone())
        .unwrap_or_default();

    let ts_start = events
        .first()
        .map(|e| e.ts.to_rfc3339())
        .unwrap_or_default();
    let ts_end = events.last().map(|e| e.ts.to_rfc3339()).unwrap_or_default();

    let tool_names = events
        .iter()
        .filter_map(|e| e.tool.as_ref().map(|t| t.name.clone()))
        .collect::<Vec<_>>();

    let error_types = events
        .iter()
        .filter(|e| e.kind == "error")
        .map(|e| e.kind.clone())
        .collect::<Vec<_>>();

    let success = !events.iter().any(|e| e.kind == "error");

    let prompt = if include_raw {
        raw_prompt
    } else {
        summarize_prompt(events)
    };

    let result = if include_raw {
        raw_result
    } else {
        summarize_result(events)
    };

    let trace = if include_raw {
        events
            .iter()
            .map(|e| EpisodeStep {
                role: role_from_kind(&e.kind).to_string(),
                content: e.text.clone(),
                name: e.tool.as_ref().map(|t| t.name.clone()),
                ts: e.ts.to_rfc3339(),
            })
            .collect::<Vec<_>>()
    } else {
        summarize_trace(events)
    };

    let context = if include_raw {
        build_context(events)
    } else {
        summarize_context(events)
    };

    let canonical = serde_json::json!({
        "source_tool": source_tool,
        "session_id": session_id,
        "ts_start": ts_start,
        "ts_end": ts_end,
        "prompt": prompt,
        "result": result,
        "trace": trace,
    });
    let canon = serde_json::to_string(&canonical).unwrap_or_default();
    let content_hash = blake3::hash(canon.as_bytes()).to_hex().to_string();
    let id = blake3::hash(format!("episode|{content_hash}").as_bytes())
        .to_hex()
        .to_string();

    if contains_sensitive_patterns(&prompt)
        || contains_sensitive_patterns(&context)
        || contains_sensitive_patterns(&result)
    {
        return None;
    }

    Some(EpisodeRecord {
        id,
        source_tool: source_tool.to_string(),
        session_id: session_id.to_string(),
        ts_start,
        ts_end,
        prompt,
        context,
        trace,
        result,
        outcome: EpisodeOutcome {
            success,
            signals: EpisodeOutcomeSignals {
                tests_passed: None,
                exit_code: extract_exit_code(events),
                lint_fixed: None,
            },
        },
        meta: EpisodeMeta {
            lang: None,
            tool_names,
            error_types,
            repo_fingerprint: None,
            os: std::env::consts::OS.parse().ok(),
            editor: None,
            raw_content_included: include_raw,
        },
        consent: EpisodeConsent {
            accepted_at: accepted_at.to_string(),
            consent_version: consent_version.to_string(),
            public_searchable: true,
            trainable: true,
        },
        license: license.to_string(),
        policy_version: policy_version.to_string(),
        sanitizer_version: sanitizer_version.to_string(),
        content_hash,
    })
}

pub fn build_episodes(
    source_tool: &str,
    session_id: &str,
    events: &[CanonicalEvent],
    include_raw: bool,
    accepted_at: &str,
    consent_version: &str,
    license: &str,
    policy_version: &str,
    sanitizer_version: &str,
) -> Vec<EpisodeRecord> {
    if events.is_empty() {
        return Vec::new();
    }

    let windows = split_event_windows(events, 300);
    windows
        .into_iter()
        .filter_map(|w| {
            build_episode(
                source_tool,
                session_id,
                &w,
                include_raw,
                accepted_at,
                consent_version,
                license,
                policy_version,
                sanitizer_version,
            )
        })
        .collect()
}

fn split_event_windows(events: &[CanonicalEvent], max_events: usize) -> Vec<Vec<CanonicalEvent>> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    let max_events = max_events.max(50);

    for event in events {
        if !current.is_empty() && (is_turn_boundary(event) || current.len() >= max_events) {
            out.push(std::mem::take(&mut current));
        }
        current.push(event.clone());
    }

    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn is_turn_boundary(event: &CanonicalEvent) -> bool {
    if event.kind == "turn_context" {
        return true;
    }
    event.kind == "user_msg" && !event.text.trim().is_empty()
}

fn summarize_prompt(events: &[CanonicalEvent]) -> String {
    if let Some(msg) = events
        .iter()
        .find(|e| e.kind == "user_msg" && !e.text.trim().is_empty())
    {
        let candidate = format!("summary_user_prompt: {}", preview(&msg.text, 220));
        if !contains_sensitive_patterns(&candidate) {
            return candidate;
        }
    }
    let user_messages = events.iter().filter(|e| e.kind == "user_msg").count();
    let meaningful_events = events
        .iter()
        .filter(|e| !is_low_signal_event(e) || !e.text.trim().is_empty())
        .count();
    format!(
        "summary: user_messages={user_messages} meaningful_events={meaningful_events} (raw prompt omitted)"
    )
}

fn summarize_result(events: &[CanonicalEvent]) -> String {
    if let Some(msg) = events.iter().rev().find(|e| {
        (e.kind == "assistant_msg" || e.kind == "response_item") && !e.text.trim().is_empty()
    }) {
        let candidate = format!("summary_assistant_result: {}", preview(&msg.text, 260));
        if !contains_sensitive_patterns(&candidate) {
            return candidate;
        }
    }
    let assistant_messages = events
        .iter()
        .filter(|e| e.kind == "assistant_msg" || e.kind == "response_item")
        .count();
    let error_events = events.iter().filter(|e| e.kind == "error").count();
    format!(
        "summary: assistant_messages={assistant_messages} error_events={error_events} (raw result omitted)"
    )
}

fn summarize_context(events: &[CanonicalEvent]) -> String {
    let mut tool_names = events
        .iter()
        .filter_map(|e| e.tool.as_ref().map(|t| t.name.clone()))
        .collect::<Vec<_>>();
    tool_names.sort();
    tool_names.dedup();
    let exit_codes = events
        .iter()
        .filter_map(|e| e.meta.as_ref().and_then(|m| m.exit_code))
        .collect::<Vec<_>>();
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    for e in events {
        *by_kind.entry(e.kind.clone()).or_insert(0) += 1;
    }
    format!(
        "summary: tools={:?} exit_codes={:?} events_by_kind={:?} error_events={}",
        tool_names,
        exit_codes,
        by_kind,
        events.iter().filter(|e| e.kind == "error").count(),
    )
}

fn summarize_trace(events: &[CanonicalEvent]) -> Vec<EpisodeStep> {
    let mut out = Vec::new();
    if let (Some(first), Some(last)) = (events.first(), events.last()) {
        let duration_secs = (last.ts - first.ts).num_seconds();
        out.push(EpisodeStep {
            role: "system".to_string(),
            content: format!(
                "summary: total_events={} duration_secs={} source_kinds_compacted=true",
                events.len(),
                duration_secs.max(0)
            ),
            name: None,
            ts: first.ts.to_rfc3339(),
        });
    }

    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    for e in events {
        *by_kind.entry(e.kind.clone()).or_insert(0) += 1;
    }
    out.push(EpisodeStep {
        role: "system".to_string(),
        content: format!("summary: by_kind={:?}", by_kind),
        name: None,
        ts: events
            .first()
            .map(|e| e.ts.to_rfc3339())
            .unwrap_or_default(),
    });

    let meaningful = events
        .iter()
        .filter(|e| !is_low_signal_event(e) || !e.text.trim().is_empty())
        .take(10);

    for e in meaningful {
        let args_len = e
            .tool
            .as_ref()
            .and_then(|t| t.args_json.as_ref())
            .map(|s| s.len())
            .unwrap_or(0);
        let result_len = e
            .tool
            .as_ref()
            .and_then(|t| t.result_json.as_ref())
            .map(|s| s.len())
            .unwrap_or(0);
        let preview_text = preview(&e.text, 140);
        let content = format!(
            "summary_event: kind={} chars={} tool_args_bytes={} tool_result_bytes={} preview=\"{}\"",
            e.kind,
            e.text.len(),
            args_len,
            result_len,
            preview_text
        );
        out.push(EpisodeStep {
            role: role_from_kind(&e.kind).to_string(),
            content,
            name: e.tool.as_ref().map(|t| t.name.clone()),
            ts: e.ts.to_rfc3339(),
        });
    }

    out
}

fn is_low_signal_event(event: &CanonicalEvent) -> bool {
    matches!(
        event.kind.as_str(),
        "event_msg" | "turn_context" | "session_meta"
    )
}

fn preview(input: &str, max_chars: usize) -> String {
    let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    compact.chars().take(max_chars).collect::<String>() + "..."
}

pub fn derive_sft(episode: &EpisodeRecord) -> SftRecord {
    SftRecord {
        id: episode.id.clone(),
        instruction: episode.prompt.clone(),
        input: episode.context.clone(),
        output: episode.result.clone(),
        meta: episode.meta.clone(),
        license: episode.license.clone(),
        consent: episode.consent.clone(),
    }
}

pub fn derive_tooltrace(episode: &EpisodeRecord) -> TooltraceRecord {
    TooltraceRecord {
        id: episode.id.clone(),
        messages: episode
            .trace
            .iter()
            .map(|s| TooltraceMessage {
                role: s.role.clone(),
                content: s.content.clone(),
                name: s.name.clone(),
            })
            .collect(),
        meta: episode.meta.clone(),
        license: episode.license.clone(),
        consent: episode.consent.clone(),
    }
}

fn role_from_kind(kind: &str) -> &str {
    match kind {
        "user_msg" => "user",
        "assistant_msg" | "response_item" => "assistant",
        "tool_call" => "assistant",
        "tool_result" => "tool",
        _ => "system",
    }
}

fn extract_exit_code(events: &[CanonicalEvent]) -> Option<i32> {
    events
        .iter()
        .rev()
        .find_map(|e| e.meta.as_ref().and_then(|m| m.exit_code))
}

fn build_context(events: &[CanonicalEvent]) -> String {
    let mut parts = Vec::new();
    let errors = events
        .iter()
        .filter(|e| e.kind == "error")
        .map(|e| e.text.clone())
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        parts.push(format!("errors: {}", errors.join(" | ")));
    }

    let constraints = events
        .iter()
        .filter(|e| e.kind == "system")
        .take(5)
        .map(|e| e.text.clone())
        .collect::<Vec<_>>();
    if !constraints.is_empty() {
        parts.push(format!("system: {}", constraints.join(" | ")));
    }

    parts.join("\n")
}

pub fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::models::CanonicalEvent;

    #[test]
    fn summary_mode_keeps_redacted_single_event() {
        let events = vec![CanonicalEvent {
            source: "x".to_string(),
            session_id: "s".to_string(),
            ts: Utc::now(),
            kind: "user_msg".to_string(),
            text: "token=[REDACTED] hello".to_string(),
            tool: None,
            meta: None,
        }];
        let ep = super::build_episode(
            "x",
            "s",
            &events,
            false,
            "2026-01-01T00:00:00Z",
            "v1",
            "CC0-1.0",
            "p1",
            "s1",
        );
        assert!(ep.is_some());
    }

    #[test]
    fn splits_large_session_into_multiple_episodes() {
        let mut events = Vec::new();
        for i in 0..620 {
            events.push(CanonicalEvent {
                source: "x".to_string(),
                session_id: "s".to_string(),
                ts: Utc::now(),
                kind: if i % 120 == 0 {
                    "user_msg".to_string()
                } else {
                    "response_item".to_string()
                },
                text: format!("event-{i}"),
                tool: None,
                meta: None,
            });
        }

        let eps = super::build_episodes("x", "s", &events, false, "a", "v1", "CC0-1.0", "p", "s");
        assert!(eps.len() >= 2);
    }
}
