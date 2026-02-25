use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use trace_share_core::{
    config::AppConfig,
    episode::{
        EpisodeConsent, EpisodeMeta, EpisodeOutcome, EpisodeOutcomeSignals, EpisodeRecord,
        EpisodeStep,
    },
    worker::upload_episode,
};

#[tokio::test]
async fn uploads_episode_via_presigned_flow() {
    let prior_insecure_http = std::env::var("TRACE_SHARE_ALLOW_INSECURE_HTTP").ok();
    unsafe {
        std::env::set_var("TRACE_SHARE_ALLOW_INSECURE_HTTP", "1");
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("local addr");
    let base = format!("http://{}", addr);
    let base_for_server = base.clone();

    let server = tokio::spawn(async move {
        for idx in 0..3 {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = vec![0u8; 32 * 1024];
            let n = socket.read(&mut buf).await.expect("read");
            let req = String::from_utf8_lossy(&buf[..n]);

            let (status, body) = if idx == 0 {
                assert!(req.starts_with("POST /v1/episodes/presign "));
                (
                    "200 OK".to_string(),
                    format!(
                        "{{\"upload_url\":\"{base_for_server}/upload/ep-1\",\"object_key\":\"episodes/ep-1.json\",\"headers\":{{\"content-type\":\"application/json\"}}}}"
                    ),
                )
            } else if idx == 1 {
                assert!(req.starts_with("PUT /upload/ep-1 "));
                ("200 OK".to_string(), String::new())
            } else {
                assert!(req.starts_with("POST /v1/episodes/complete "));
                (
                    "200 OK".to_string(),
                    "{\"episode_id\":\"ep-1\",\"object_key\":\"episodes/ep-1.json\",\"etag\":\"etag-1\"}"
                        .to_string(),
                )
            };

            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });

    let mut cfg = AppConfig::default();
    cfg.worker.base_url = Some(base);
    cfg.worker.upload_mode = "presigned".to_string();

    let episode = sample_episode();
    let result = upload_episode(&cfg, &episode)
        .await
        .expect("upload succeeds");
    assert_eq!(result.episode_id, "ep-1");
    assert_eq!(result.object_key, "episodes/ep-1.json");
    assert_eq!(result.etag.as_deref(), Some("etag-1"));

    server.await.expect("server done");

    unsafe {
        if let Some(v) = prior_insecure_http {
            std::env::set_var("TRACE_SHARE_ALLOW_INSECURE_HTTP", v);
        } else {
            std::env::remove_var("TRACE_SHARE_ALLOW_INSECURE_HTTP");
        }
    }
}

fn sample_episode() -> EpisodeRecord {
    EpisodeRecord {
        id: "ep-1".to_string(),
        source_tool: "codex_cli".to_string(),
        session_id: "session-1".to_string(),
        ts_start: "2026-02-25T00:00:00Z".to_string(),
        ts_end: "2026-02-25T00:01:00Z".to_string(),
        prompt: "prompt".to_string(),
        context: "context".to_string(),
        trace: vec![EpisodeStep {
            role: "assistant".to_string(),
            content: "content".to_string(),
            name: None,
            ts: "2026-02-25T00:00:30Z".to_string(),
        }],
        result: "result".to_string(),
        outcome: EpisodeOutcome {
            success: true,
            signals: EpisodeOutcomeSignals {
                tests_passed: Some(true),
                exit_code: Some(0),
                lint_fixed: Some(true),
            },
        },
        meta: EpisodeMeta {
            lang: Some("rust".to_string()),
            tool_names: vec!["shell".to_string()],
            error_types: vec![],
            repo_fingerprint: Some("repo".to_string()),
            os: Some("linux".to_string()),
            editor: Some("vscode".to_string()),
            raw_content_included: false,
        },
        consent: EpisodeConsent {
            accepted_at: "2026-02-25T00:00:00Z".to_string(),
            consent_version: "v1".to_string(),
            public_searchable: true,
            trainable: true,
        },
        license: "CC0-1.0".to_string(),
        policy_version: "policy-v1".to_string(),
        sanitizer_version: "sanitizer-v1".to_string(),
        content_hash: "hash-1".to_string(),
    }
}
