use std::{fs, path::Path};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use trace_share_core::{config::AppConfig, snapshot::publish_snapshot};

#[tokio::test]
async fn snapshot_publish_dry_run_validates_artifacts_without_network() {
    let root = std::env::temp_dir().join(format!(
        "trace-share-snapshot-publish-{}",
        uuid::Uuid::new_v4()
    ));
    let snapshot_dir = root.join("dataset-1.0.0");
    create_snapshot_layout(&snapshot_dir);

    let cfg = AppConfig::default();
    let out = publish_snapshot(&cfg, "1.0.0", &root, true)
        .await
        .expect("dry-run publish should succeed");

    assert_eq!(out.version, "1.0.0");
    assert_eq!(out.snapshot_dir, snapshot_dir);
    assert!(!out.indexed);
    assert!(out.object_prefix.is_none());
}

#[tokio::test]
async fn snapshot_publish_uploads_and_indexes() {
    let root = std::env::temp_dir().join(format!(
        "trace-share-snapshot-publish-{}",
        uuid::Uuid::new_v4()
    ));
    let snapshot_dir = root.join("dataset-2.0.0");
    create_snapshot_layout(&snapshot_dir);

    let (worker_url, worker_task) = start_worker_server(200).await;
    let (upstash_url, upstash_task) = start_upstash_server(200).await;

    let mut cfg = AppConfig::default();
    cfg.worker.base_url = Some(worker_url);
    cfg.upstash.rest_url = Some(upstash_url);
    cfg.upstash.rest_token = Some("token".to_string());

    let out = publish_snapshot(&cfg, "2.0.0", &root, false)
        .await
        .expect("publish should succeed");
    assert!(out.indexed);
    assert_eq!(out.object_prefix.as_deref(), Some("datasets/dataset-2.0.0"));

    worker_task.await.expect("worker task");
    upstash_task.await.expect("upstash task");
}

fn create_snapshot_layout(snapshot_dir: &Path) {
    fs::create_dir_all(snapshot_dir).expect("create snapshot dir");
    fs::write(snapshot_dir.join("train.jsonl.zst"), b"train").expect("write train");
    fs::write(snapshot_dir.join("val.jsonl.zst"), b"val").expect("write val");
    fs::write(snapshot_dir.join("sft.jsonl.zst"), b"sft").expect("write sft");
    fs::write(snapshot_dir.join("tooltrace.jsonl.zst"), b"tooltrace").expect("write tooltrace");
    fs::write(
        snapshot_dir.join("manifest.json"),
        r#"{"version":"test","total_records":1,"train_count":1,"val_count":0}"#,
    )
    .expect("write manifest");
    fs::write(snapshot_dir.join("CHECKSUMS.txt"), "dummy").expect("write checksums");
    fs::write(snapshot_dir.join("DATA_CARD.md"), "# DATA_CARD").expect("write data card");
}

async fn start_worker_server(status: u16) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("local addr");
    let url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept connection");
        let mut buf = vec![0u8; 16384];
        let _ = socket.read(&mut buf).await;

        let status_text = if status == 200 {
            "OK"
        } else {
            "Internal Server Error"
        };
        let body = if status == 200 {
            r#"{"version":"2.0.0","object_prefix":"datasets/dataset-2.0.0","public_url":"https://example"}"#
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

        let status_text = if status == 200 {
            "OK"
        } else {
            "Internal Server Error"
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
