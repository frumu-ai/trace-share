use anyhow::{Context, Result};
use rand::{Rng, thread_rng};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tokio::time::{Duration, sleep};

use crate::security::ensure_secure_url;
use crate::{config::AppConfig, episode::EpisodeRecord};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerUploadResponse {
    pub episode_id: String,
    pub object_key: String,
    pub etag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresignUploadResponse {
    upload_url: String,
    object_key: String,
    headers: Option<BTreeMap<String, String>>,
}

pub async fn upload_episode(
    config: &AppConfig,
    episode: &EpisodeRecord,
) -> Result<WorkerUploadResponse> {
    let mode = config.worker.upload_mode.to_ascii_lowercase();
    if mode == "presigned" {
        return upload_episode_presigned(config, episode).await;
    }
    if mode == "auto" {
        match upload_episode_presigned(config, episode).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let text = format!("{e:#}");
                if !(text.contains("status=404") || text.contains("status=501")) {
                    return Err(e).context("worker upload (auto mode, presigned path)");
                }
            }
        }
    }
    upload_episode_legacy(config, episode).await
}

async fn upload_episode_legacy(
    config: &AppConfig,
    episode: &EpisodeRecord,
) -> Result<WorkerUploadResponse> {
    let base_url = config
        .worker
        .base_url
        .as_ref()
        .context("missing TRACE_SHARE_WORKER_BASE_URL")?;
    ensure_secure_url(base_url, "worker base URL")?;

    let endpoint = format!("{}/v1/episodes", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.worker.timeout_seconds.max(5),
        ))
        .build()?;

    let mut attempt: u32 = 0;
    loop {
        let mut req = client.post(&endpoint).json(episode);
        if let Some(token) = config.worker.api_token.as_ref() {
            req = req.bearer_auth(token);
        }

        let resp = req.send().await;
        match resp {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    return Ok(resp.json::<WorkerUploadResponse>().await?);
                }

                let body = resp.text().await.unwrap_or_default();
                if !should_retry_status(status) || attempt >= 4 {
                    anyhow::bail!("worker upload failed: status={} body={}", status, body);
                }
            }
            Err(e) => {
                let retryable_transport = e.is_timeout() || e.is_connect() || e.is_request();
                if !retryable_transport || attempt >= 4 {
                    return Err(e).context("worker upload request failed after retries");
                }
            }
        }

        attempt += 1;
        let jitter: u64 = thread_rng().gen_range(50..300);
        let wait_ms = (2u64.pow(attempt) * 200) + jitter;
        sleep(Duration::from_millis(wait_ms)).await;
    }
}

async fn upload_episode_presigned(
    config: &AppConfig,
    episode: &EpisodeRecord,
) -> Result<WorkerUploadResponse> {
    let base_url = config
        .worker
        .base_url
        .as_ref()
        .context("missing TRACE_SHARE_WORKER_BASE_URL")?;
    ensure_secure_url(base_url, "worker base URL")?;
    let presign_endpoint = format!("{}/v1/episodes/presign", base_url.trim_end_matches('/'));
    let complete_endpoint = format!("{}/v1/episodes/complete", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.worker.timeout_seconds.max(5),
        ))
        .build()?;

    let presign_payload = serde_json::json!({
        "episode_id": episode.id,
        "content_hash": episode.content_hash,
        "content_type": "application/json",
    });

    let presign = post_with_retry_json::<PresignUploadResponse>(
        &client,
        &presign_endpoint,
        config.worker.api_token.as_deref(),
        &presign_payload,
        "worker episode presign",
    )
    .await?;

    let episode_bytes = serde_json::to_vec(episode)?;
    put_with_retry(
        &client,
        &presign.upload_url,
        presign.headers.as_ref(),
        &episode_bytes,
        "worker episode upload",
    )
    .await?;

    let complete_payload = serde_json::json!({
        "episode_id": episode.id,
        "object_key": presign.object_key,
        "content_hash": episode.content_hash,
    });

    post_with_retry_json::<WorkerUploadResponse>(
        &client,
        &complete_endpoint,
        config.worker.api_token.as_deref(),
        &complete_payload,
        "worker episode complete",
    )
    .await
}

pub async fn push_revocation(
    config: &AppConfig,
    episode_id: &str,
    revoked_at: &str,
    reason: Option<&str>,
) -> Result<()> {
    let base_url = config
        .worker
        .base_url
        .as_ref()
        .context("missing TRACE_SHARE_WORKER_BASE_URL")?;
    ensure_secure_url(base_url, "worker base URL")?;

    let endpoint = format!("{}/v1/revocations", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.worker.timeout_seconds.max(5),
        ))
        .build()?;

    let payload = serde_json::json!({
        "episode_id": episode_id,
        "revoked_at": revoked_at,
        "reason": reason,
    });

    let mut attempt: u32 = 0;
    loop {
        let mut req = client.post(&endpoint).json(&payload);
        if let Some(token) = config.worker.api_token.as_ref() {
            req = req.bearer_auth(token);
        }

        let resp = req.send().await;
        match resp {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    return Ok(());
                }

                let body = resp.text().await.unwrap_or_default();
                if !should_retry_status(status) || attempt >= 4 {
                    anyhow::bail!(
                        "worker revocation push failed: status={} body={}",
                        status,
                        body
                    );
                }
            }
            Err(e) => {
                let retryable_transport = e.is_timeout() || e.is_connect() || e.is_request();
                if !retryable_transport || attempt >= 4 {
                    return Err(e).context("worker revocation request failed after retries");
                }
            }
        }

        attempt += 1;
        let jitter: u64 = thread_rng().gen_range(50..300);
        let wait_ms = (2u64.pow(attempt) * 200) + jitter;
        sleep(Duration::from_millis(wait_ms)).await;
    }
}

async fn post_with_retry_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    endpoint: &str,
    bearer_token: Option<&str>,
    payload: &serde_json::Value,
    label: &str,
) -> Result<T> {
    let mut attempt: u32 = 0;
    loop {
        let mut req = client.post(endpoint).json(payload);
        if let Some(token) = bearer_token {
            req = req.bearer_auth(token);
        }

        let resp = req.send().await;
        match resp {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    return Ok(resp.json::<T>().await?);
                }
                let body = resp.text().await.unwrap_or_default();
                if !should_retry_status(status) || attempt >= 4 {
                    anyhow::bail!("{label} failed: status={} body={}", status, body);
                }
            }
            Err(e) => {
                let retryable_transport = e.is_timeout() || e.is_connect() || e.is_request();
                if !retryable_transport || attempt >= 4 {
                    return Err(e).with_context(|| format!("{label} request failed after retries"));
                }
            }
        }

        attempt += 1;
        let jitter: u64 = thread_rng().gen_range(50..300);
        let wait_ms = (2u64.pow(attempt) * 200) + jitter;
        sleep(Duration::from_millis(wait_ms)).await;
    }
}

async fn put_with_retry(
    client: &reqwest::Client,
    endpoint: &str,
    headers: Option<&BTreeMap<String, String>>,
    body: &[u8],
    label: &str,
) -> Result<()> {
    ensure_secure_url(endpoint, label)?;
    let mut attempt: u32 = 0;
    loop {
        let mut req = client.put(endpoint).body(body.to_vec());
        if let Some(headers) = headers {
            for (k, v) in headers {
                req = req.header(k, v);
            }
        }

        let resp = req.send().await;
        match resp {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    return Ok(());
                }
                let body = resp.text().await.unwrap_or_default();
                if !should_retry_status(status) || attempt >= 4 {
                    anyhow::bail!("{label} failed: status={} body={}", status, body);
                }
            }
            Err(e) => {
                let retryable_transport = e.is_timeout() || e.is_connect() || e.is_request();
                if !retryable_transport || attempt >= 4 {
                    return Err(e).with_context(|| format!("{label} request failed after retries"));
                }
            }
        }

        attempt += 1;
        let jitter: u64 = thread_rng().gen_range(50..300);
        let wait_ms = (2u64.pow(attempt) * 200) + jitter;
        sleep(Duration::from_millis(wait_ms)).await;
    }
}

fn should_retry_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}
