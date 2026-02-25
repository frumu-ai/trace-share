use crate::models::{
    CanonicalEvent, ChunkDocument, ChunkMetadata, content_hash, doc_id, normalize_text,
};

const CHUNK_CHAR_LIMIT: usize = 3200;

pub fn chunk_events(
    events: &[CanonicalEvent],
    policy_version: &str,
    sanitizer_version: &str,
) -> Vec<ChunkDocument> {
    if events.is_empty() {
        return Vec::new();
    }

    let mut docs = Vec::new();
    let mut bucket: Vec<&CanonicalEvent> = Vec::new();
    let mut bucket_size = 0usize;
    let mut chunk_index = 0usize;

    for event in events {
        let piece = format!("[{}][{}] {}", event.ts.to_rfc3339(), event.kind, event.text);
        if !bucket.is_empty() && bucket_size + piece.len() > CHUNK_CHAR_LIMIT {
            docs.push(make_doc(
                &bucket,
                chunk_index,
                policy_version,
                sanitizer_version,
            ));
            bucket.clear();
            bucket_size = 0;
            chunk_index += 1;
        }
        bucket_size += piece.len();
        bucket.push(event);
    }

    if !bucket.is_empty() {
        docs.push(make_doc(
            &bucket,
            chunk_index,
            policy_version,
            sanitizer_version,
        ));
    }

    docs
}

fn make_doc(
    events: &[&CanonicalEvent],
    chunk_index: usize,
    policy_version: &str,
    sanitizer_version: &str,
) -> ChunkDocument {
    let source = events[0].source.clone();
    let session_id = events[0].session_id.clone();
    let ts_start = events[0].ts.to_rfc3339();
    let ts_end = events[events.len() - 1].ts.to_rfc3339();

    let text = events
        .iter()
        .map(|e| format!("[{}][{}] {}", e.ts.to_rfc3339(), e.kind, e.text))
        .collect::<Vec<_>>()
        .join("\n");

    let tool_names = events
        .iter()
        .filter_map(|e| e.tool.as_ref().map(|t| t.name.clone()))
        .collect::<Vec<_>>();

    let error_types = events
        .iter()
        .filter(|e| e.kind == "error")
        .map(|e| e.kind.clone())
        .collect::<Vec<_>>();

    let normalized = normalize_text(&text);
    let c_hash = content_hash(&normalized);
    let id = doc_id(&source, &session_id, chunk_index, &c_hash);

    ChunkDocument {
        id,
        text,
        metadata: ChunkMetadata {
            source,
            session_id,
            chunk_index,
            ts_start,
            ts_end,
            tool_names,
            error_types,
            repo_fingerprint: None,
            language: None,
            policy_version: policy_version.to_string(),
            sanitizer_version: sanitizer_version.to_string(),
            content_hash: c_hash,
        },
    }
}
