use trace_share_core::{chunk::chunk_events, models::CanonicalEvent};

#[test]
fn same_input_produces_same_doc_ids() {
    let events = vec![CanonicalEvent {
        source: "codex_cli".into(),
        session_id: "session-1".into(),
        ts: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
        kind: "user_msg".into(),
        text: "hello".into(),
        tool: None,
        meta: None,
    }];

    let a = chunk_events(&events, "policy-v1", "san-v1");
    let b = chunk_events(&events, "policy-v1", "san-v1");
    assert_eq!(a[0].id, b[0].id);
    assert_eq!(a[0].metadata.content_hash, b[0].metadata.content_hash);
}
