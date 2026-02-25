use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub args_json: Option<String>,
    pub result_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMeta {
    pub cwd: Option<String>,
    pub repo: Option<String>,
    pub exit_code: Option<i32>,
    pub model: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalEvent {
    pub source: String,
    pub session_id: String,
    pub ts: DateTime<Utc>,
    pub kind: String,
    pub text: String,
    pub tool: Option<ToolInfo>,
    pub meta: Option<EventMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub source: String,
    pub session_id: String,
    pub chunk_index: usize,
    pub ts_start: String,
    pub ts_end: String,
    pub tool_names: Vec<String>,
    pub error_types: Vec<String>,
    pub repo_fingerprint: Option<String>,
    pub language: Option<String>,
    pub policy_version: String,
    pub sanitizer_version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkDocument {
    pub id: String,
    pub text: String,
    pub metadata: ChunkMetadata,
}

pub fn normalize_text(input: &str) -> String {
    input
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn content_hash(normalized_sanitized_text: &str) -> String {
    blake3::hash(normalized_sanitized_text.as_bytes())
        .to_hex()
        .to_string()
}

pub fn doc_id(source: &str, session_id: &str, chunk_index: usize, content_hash: &str) -> String {
    let seed = format!("{source}|{session_id}|{chunk_index}|{content_hash}");
    blake3::hash(seed.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::{content_hash, doc_id, normalize_text};

    #[test]
    fn deterministic_hash_and_id() {
        let normalized = normalize_text("hello\nworld\n");
        let h1 = content_hash(&normalized);
        let h2 = content_hash(&normalized);
        assert_eq!(h1, h2);

        let d1 = doc_id("codex_cli", "abc", 1, &h1);
        let d2 = doc_id("codex_cli", "abc", 1, &h1);
        assert_eq!(d1, d2);
    }
}
