use std::fs;

use serial_test::serial;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use trace_share_core::{config::AppConfig, revocation::sync_revocations, state::StateStore};

#[tokio::test]
#[serial]
async fn revocation_sync_retries_transient_and_marks_pushed() {
    let prior_insecure_http = std::env::var("TRACE_SHARE_ALLOW_INSECURE_HTTP").ok();
    unsafe {
        std::env::set_var("TRACE_SHARE_ALLOW_INSECURE_HTTP", "1");
    }

    let (base_url, server_task) = start_revocation_server(vec![500, 200]).await;
    let db_path = temp_db_path("revocation-sync-ok");
    let store = StateStore::open(db_path).expect("open state");

    store
        .upsert_revocation(
            "episode-1",
            Some("user request"),
            "2026-02-25T00:00:00Z",
            "pending",
        )
        .expect("seed revocation");

    let mut cfg = AppConfig::default();
    cfg.worker.base_url = Some(base_url);

    let pushed = sync_revocations(&cfg, &store)
        .await
        .expect("sync should succeed");
    assert_eq!(pushed, 1);

    let pending = store.pending_revocations().expect("query pending");
    assert!(pending.is_empty());

    server_task.await.expect("server task");

    unsafe {
        if let Some(v) = prior_insecure_http {
            std::env::set_var("TRACE_SHARE_ALLOW_INSECURE_HTTP", v);
        } else {
            std::env::remove_var("TRACE_SHARE_ALLOW_INSECURE_HTTP");
        }
    }
}

#[tokio::test]
#[serial]
async fn revocation_sync_does_not_retry_non_transient_and_keeps_pending() {
    let prior_insecure_http = std::env::var("TRACE_SHARE_ALLOW_INSECURE_HTTP").ok();
    unsafe {
        std::env::set_var("TRACE_SHARE_ALLOW_INSECURE_HTTP", "1");
    }

    let (base_url, server_task) = start_revocation_server(vec![400]).await;
    let db_path = temp_db_path("revocation-sync-fail");
    let store = StateStore::open(db_path).expect("open state");

    store
        .upsert_revocation("episode-2", None, "2026-02-25T00:00:00Z", "pending")
        .expect("seed revocation");

    let mut cfg = AppConfig::default();
    cfg.worker.base_url = Some(base_url);

    let err = sync_revocations(&cfg, &store)
        .await
        .expect_err("sync should fail on non-transient 400");
    assert!(format!("{err:#}").contains("status=400"));

    let pending = store.pending_revocations().expect("query pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].episode_id, "episode-2");

    server_task.await.expect("server task");

    unsafe {
        if let Some(v) = prior_insecure_http {
            std::env::set_var("TRACE_SHARE_ALLOW_INSECURE_HTTP", v);
        } else {
            std::env::remove_var("TRACE_SHARE_ALLOW_INSECURE_HTTP");
        }
    }
}

fn temp_db_path(prefix: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create temp root");
    root.join("state.sqlite")
}

async fn start_revocation_server(statuses: Vec<u16>) -> (String, tokio::task::JoinHandle<()>) {
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
            let body = if status == 200 { "{\"ok\":true}" } else { "" };
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
