use std::fs;

use serial_test::serial;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use trace_share_core::{
    config::AppConfig,
    consent::init_consent,
    pipeline::{RunOptions, run_once},
    state::StateStore,
};

#[tokio::test]
#[serial]
async fn retries_transient_failures_and_persists_upload_state() {
    let (worker_base_url, worker_task) = start_worker_server(vec![500, 500, 200]).await;
    let (upstash_base_url, upstash_task) = start_upstash_server(200).await;

    let home = std::env::temp_dir().join(format!("trace-share-itest-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&home).expect("create temp home");
    let source_root = home.join("source");
    fs::create_dir_all(&source_root).expect("create source root");
    let log_file = source_root.join("session.jsonl");

    fs::write(
        &log_file,
        "{\"ts\":\"2026-02-25T00:00:00Z\",\"kind\":\"user_msg\",\"text\":\"token=abc123 hello\"}\n",
    )
    .expect("write input log");

    let sources_path = home.join("sources.toml");
    fs::write(
        &sources_path,
        format!(
            "version = 1\n\n[[sources]]\nid = \"itest_source\"\nroots = [\"{}\"]\nglobs = [\"**/*.jsonl\"]\nformat = \"jsonl\"\nrequires_opt_in = false\n",
            source_root.display()
        ),
    )
    .expect("write sources manifest");

    let prior_home = std::env::var("HOME").ok();
    let prior_insecure_http = std::env::var("TRACE_SHARE_ALLOW_INSECURE_HTTP").ok();
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("TRACE_SHARE_HOME", &home);
        std::env::set_var("TRACE_SHARE_ALLOW_INSECURE_HTTP", "1");
    }

    let mut cfg = AppConfig::default();
    cfg.worker.base_url = Some(worker_base_url);
    cfg.upstash.rest_url = Some(upstash_base_url);
    cfg.upstash.rest_token = Some("test-token".to_string());
    cfg.sources_path = Some(sources_path);
    cfg.remote_registry.enabled = false;

    let state = StateStore::open_default().expect("open state");
    init_consent(&state, "CC0-1.0", "test-consent-v1").expect("init consent");
    let export_path = home.join("exported.jsonl");

    let result = run_once(
        &cfg,
        &RunOptions {
            sources: vec!["itest_source".to_string()],
            dry_run: false,
            review: true,
            yes: true,
            include_raw: false,
            show_payload: false,
            preview_limit: 1,
            explain_size: true,
            export_payload_path: Some(export_path.clone()),
            export_limit: None,
            max_upload_bytes: None,
        },
    )
    .await
    .expect("run_once should succeed");

    assert_eq!(result.uploaded_docs, 1);
    let exported = fs::read_to_string(&export_path).expect("read exported payload");
    assert!(!exported.trim().is_empty());
    assert!(result.exported_payload_docs >= 1);

    let state = StateStore::open_default().expect("open state");
    let totals = state
        .episode_totals_by_source()
        .expect("query episode totals");
    assert!(
        totals
            .iter()
            .any(|(source, count)| source == "itest_source" && *count == 1)
    );

    unsafe {
        if let Some(v) = prior_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
        std::env::remove_var("TRACE_SHARE_HOME");
        if let Some(v) = prior_insecure_http {
            std::env::set_var("TRACE_SHARE_ALLOW_INSECURE_HTTP", v);
        } else {
            std::env::remove_var("TRACE_SHARE_ALLOW_INSECURE_HTTP");
        }
    }

    worker_task.await.expect("worker task should complete");
    upstash_task.await.expect("upstash task should complete");
}

#[tokio::test]
#[serial]
async fn does_not_retry_non_transient_400_errors() {
    let (worker_base_url, worker_task) = start_worker_server(vec![400]).await;

    let home = std::env::temp_dir().join(format!("trace-share-itest-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&home).expect("create temp home");
    let source_root = home.join("source");
    fs::create_dir_all(&source_root).expect("create source root");
    let log_file = source_root.join("session.jsonl");

    fs::write(
        &log_file,
        "{\"ts\":\"2026-02-25T00:00:00Z\",\"kind\":\"user_msg\",\"text\":\"hello world\"}\n",
    )
    .expect("write input log");

    let sources_path = home.join("sources.toml");
    fs::write(
        &sources_path,
        format!(
            "version = 1\n\n[[sources]]\nid = \"itest_source\"\nroots = [\"{}\"]\nglobs = [\"**/*.jsonl\"]\nformat = \"jsonl\"\nrequires_opt_in = false\n",
            source_root.display()
        ),
    )
    .expect("write sources manifest");

    let prior_home = std::env::var("HOME").ok();
    let prior_insecure_http = std::env::var("TRACE_SHARE_ALLOW_INSECURE_HTTP").ok();
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("TRACE_SHARE_HOME", &home);
        std::env::set_var("TRACE_SHARE_ALLOW_INSECURE_HTTP", "1");
    }

    let mut cfg = AppConfig::default();
    cfg.worker.base_url = Some(worker_base_url);
    cfg.upstash.rest_url = Some("http://127.0.0.1:9".to_string());
    cfg.upstash.rest_token = Some("test-token".to_string());
    cfg.sources_path = Some(sources_path);
    cfg.remote_registry.enabled = false;

    let state = StateStore::open_default().expect("open state");
    init_consent(&state, "CC0-1.0", "test-consent-v1").expect("init consent");

    let err = run_once(
        &cfg,
        &RunOptions {
            sources: vec!["itest_source".to_string()],
            dry_run: false,
            review: true,
            yes: true,
            include_raw: false,
            show_payload: false,
            preview_limit: 1,
            explain_size: false,
            export_payload_path: None,
            export_limit: None,
            max_upload_bytes: None,
        },
    )
    .await
    .expect_err("run_once should fail on 400 without retries");

    let err_text = format!("{err:#}");
    assert!(err_text.contains("status=400"));

    unsafe {
        if let Some(v) = prior_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
        std::env::remove_var("TRACE_SHARE_HOME");
        if let Some(v) = prior_insecure_http {
            std::env::set_var("TRACE_SHARE_ALLOW_INSECURE_HTTP", v);
        } else {
            std::env::remove_var("TRACE_SHARE_ALLOW_INSECURE_HTTP");
        }
    }

    worker_task.await.expect("worker task should complete");
}

async fn start_worker_server(statuses: Vec<u16>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("local addr");
    let url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        for status in statuses {
            let (mut socket, _) = listener.accept().await.expect("accept connection");
            let mut buf = vec![0u8; 8192];
            let _ = socket.read(&mut buf).await;

            let status_text = match status {
                200 => "OK",
                400 => "Bad Request",
                429 => "Too Many Requests",
                500 => "Internal Server Error",
                _ => "Error",
            };
            let body = if status == 200 {
                "{\"episode_id\":\"ep-test\",\"object_key\":\"episodes/ep-test.json\",\"etag\":\"etag-1\"}"
            } else {
                ""
            };
            let response = format!(
                "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n{}",
                status,
                status_text,
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });

    (url, handle)
}

async fn start_upstash_server(status: u16) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("local addr");
    let url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept connection");
        let mut buf = vec![0u8; 8192];
        let _ = socket.read(&mut buf).await;

        let status_text = match status {
            200 => "OK",
            400 => "Bad Request",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            _ => "Error",
        };
        let body = if status == 200 {
            "{\"result\":\"ok\"}"
        } else {
            ""
        };
        let response = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n{}",
            status,
            status_text,
            body.len(),
            body
        );
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.shutdown().await;
    });

    (url, handle)
}
